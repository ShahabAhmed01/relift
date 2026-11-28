//! Read-only imaging of a source to a new file.
//!
//! The destination is opened with `create_new`; an existing file is never
//! overwritten. The image is written to `<dest>.partial` and renamed only after
//! a successful flush. With verification enabled (the default in the CLI) the
//! finished image is re-read and hashed, and a sidecar manifest records both the
//! source identity and the image digest.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::hash::{hash_reader, Hasher};
use crate::safety::{self, DestinationDirectory};
use crate::session::now_iso;
use crate::source::{files_are_same, ReadOnlySource, SourceKind};
use crate::VERSION;

const COPY_BLOCK: usize = 8 * 1024 * 1024;
const DEFAULT_CHECKPOINT_INTERVAL: u64 = 256 * 1024 * 1024;
const IMAGE_MAP_SCHEMA: u32 = 1;

#[derive(Debug, Clone)]
pub struct ImageOptions {
    pub verify: bool,
    /// Continue an interrupted copy from its last durable checkpoint.
    pub resume: bool,
    /// Bytes copied between durable checkpoints. Smaller values reduce the
    /// amount recopied after interruption at the cost of more sync operations.
    pub checkpoint_interval_bytes: u64,
    pub cancelled: Arc<AtomicBool>,
}

impl Default for ImageOptions {
    fn default() -> Self {
        Self {
            verify: true,
            resume: false,
            checkpoint_interval_bytes: DEFAULT_CHECKPOINT_INTERVAL,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageManifest {
    pub schema: u32,
    pub tool: String,
    pub tool_version: String,
    pub created: String,
    pub source: crate::source::SourceIdentity,
    pub image_path: String,
    pub bytes: u64,
    pub sha256: String,
    pub verified: bool,
    pub resumed: bool,
    pub verification: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageResult {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub verified: bool,
    pub resumed: bool,
    pub manifest_path: Option<PathBuf>,
    pub elapsed_seconds: f64,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum ImageMapRecord {
    Header {
        schema: u32,
        source: crate::source::SourceIdentity,
    },
    Checkpoint {
        offset: u64,
        sha256: String,
    },
}

#[derive(Debug)]
struct ImageCheckpoint {
    offset: u64,
    sha256: String,
}

/// Copy `src` to a new image file. The caller must have chosen a destination
/// directory; this function proves it is safe relative to the source identity.
pub fn create_image(
    src: &dyn ReadOnlySource,
    dest: &Path,
    options: &ImageOptions,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ImageResult> {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let canonical_parent = safety::validate_destination(&src.identity(), parent)?;
    let destination_root = DestinationDirectory::open(&canonical_parent)?;
    let image_name = PathBuf::from(dest.file_name().ok_or_else(|| {
        Error::UnsafeDestination(format!(
            "image destination {} does not name a file",
            dest.display()
        ))
    })?);
    let partial_name = suffixed_name(&image_name, ".partial");
    let map_name = suffixed_name(&image_name, ".relift-map.jsonl");
    let manifest_name = suffixed_name(&image_name, ".relift.json");
    let source_identity = src.identity();
    let image_exists = destination_root.entry_exists(&image_name)?;
    let manifest_exists = destination_root.entry_exists(&manifest_name)?;
    if manifest_exists {
        return Err(Error::UnsafeDestination(format!(
            "{} already exists; refusing to overwrite it",
            canonical_parent.join(&manifest_name).display()
        )));
    }
    if image_exists {
        let partial_exists = destination_root.entry_exists(&partial_name)?;
        let map_exists = destination_root.entry_exists(&map_name)?;
        if options.resume && !partial_exists && map_exists {
            return resume_finalized_image(
                &destination_root,
                &image_name,
                &map_name,
                &manifest_name,
                src,
                src.len(),
            );
        }
        return Err(Error::UnsafeDestination(format!(
            "{} already exists; refusing to overwrite it",
            canonical_parent.join(&image_name).display()
        )));
    }
    let image_path = canonical_parent.join(&image_name);
    let partial = canonical_parent.join(&partial_name);
    let started = std::time::Instant::now();
    let total = src.len();

    let (mut partial_output, mut map_output, mut at, mut hasher, resumed, mut checkpoint_at) =
        if options.resume {
            if !destination_root.entry_exists(&partial_name)?
                || !destination_root.entry_exists(&map_name)?
            {
                return Err(Error::UnsafeDestination(format!(
                    "resume requires both {} and {}; start a new image without --resume",
                    partial.display(),
                    canonical_parent.join(&map_name).display()
                )));
            }
            let mut map = destination_root.open_existing_file(&map_name)?;
            let map_file = map.file.as_mut().ok_or_else(|| {
                Error::Other("opened image map did not contain a file handle".into())
            })?;
            if src.is_same_file(map_file)? {
                return Err(Error::UnsafeDestination(
                    "image checkpoint map is the same underlying file as the read-only source; refusing to resume"
                        .into(),
                ));
            }
            let mut partial_output = destination_root.open_existing_file(&partial_name)?;
            let partial_file = partial_output.file.as_mut().ok_or_else(|| {
                Error::Other("opened partial image did not contain a file handle".into())
            })?;
            if src.is_same_file(partial_file)? {
                return Err(Error::UnsafeDestination(
                    "partial image is the same underlying file as the read-only source; refusing to resume"
                        .into(),
                ));
            }
            if files_are_same(map_file, partial_file)? {
                return Err(Error::UnsafeDestination(
                    "image checkpoint map and partial image are the same underlying file; refusing to resume"
                        .into(),
                ));
            }
            let checkpoint = load_map(map_file, &source_identity, total)?;
            let partial_len = partial_file
                .metadata()
                .map_err(|source| Error::Io { offset: 0, source })?
                .len();
            if partial_len < checkpoint.offset {
                return Err(Error::Session(format!(
                    "partial image is {partial_len} bytes, shorter than the last durable checkpoint at {} bytes",
                    checkpoint.offset
                )));
            }
            let hasher = hash_prefix(partial_file, checkpoint.offset)?;
            if hasher.snapshot_hex() != checkpoint.sha256 {
                return Err(Error::Session(
                    "partial image does not match its last durable checkpoint; refusing to resume"
                        .into(),
                ));
            }
            if hash_source_prefix(src, checkpoint.offset)?.snapshot_hex() != checkpoint.sha256 {
                return Err(Error::SourceMismatch(
                    "source bytes before the last durable image checkpoint changed; refusing to resume"
                        .into(),
                ));
            }
            partial_file
                .set_len(checkpoint.offset)
                .map_err(|source| Error::Io {
                    offset: checkpoint.offset,
                    source,
                })?;
            partial_file
                .seek(SeekFrom::Start(checkpoint.offset))
                .map_err(|source| Error::Io {
                    offset: checkpoint.offset,
                    source,
                })?;
            (
                partial_output,
                map,
                checkpoint.offset,
                hasher,
                true,
                checkpoint.offset,
            )
        } else {
            if destination_root.entry_exists(&partial_name)?
                || destination_root.entry_exists(&map_name)?
            {
                return Err(Error::UnsafeDestination(format!(
                    "{} or {} already exists; use --resume to continue a Relift image",
                    canonical_parent.join(&partial_name).display(),
                    canonical_parent.join(&map_name).display()
                )));
            }
            let partial_output = destination_root.create_new_file(&partial_name)?;
            let mut map = match destination_root.create_new_file(&map_name) {
                Ok(map) => map,
                Err(error) => {
                    partial_output.remove();
                    return Err(error);
                }
            };
            let map_file = map.file.as_mut().ok_or_else(|| {
                Error::Other("new image map did not contain an open file handle".into())
            })?;
            write_map_header(map_file, &source_identity)?;
            (partial_output, map, 0, Hasher::new(), false, 0)
        };

    let mut out = partial_output.file.take().ok_or_else(|| {
        Error::Other("new partial image did not contain an open file handle".into())
    })?;
    let map_file = map_output
        .file
        .as_mut()
        .ok_or_else(|| Error::Other("image map did not contain an open file handle".into()))?;
    progress(at, total);
    let mut buf = vec![0u8; COPY_BLOCK];
    let mut warnings = Vec::new();
    while at < total {
        if options.cancelled.load(Ordering::Relaxed) {
            out.flush()
                .map_err(|source| Error::Io { offset: at, source })?;
            out.sync_all()
                .map_err(|source| Error::Io { offset: at, source })?;
            save_checkpoint(map_file, at, &hasher, &mut checkpoint_at)?;
            return Err(Error::Cancelled);
        }
        let want = ((total - at) as usize).min(buf.len());
        let n = src.read_at(at, &mut buf[..want]).map_err(|e| Error::Io {
            offset: at,
            source: std::io::Error::other(e.to_string()),
        })?;
        if n == 0 {
            return Err(Error::Other(format!(
                "source ended early at {at:#x} while copying {total} bytes; partial image left at {}",
                partial.display()
            )));
        }
        out.write_all(&buf[..n]).map_err(|e| Error::Io {
            offset: at,
            source: e,
        })?;
        hasher.update(&buf[..n]);
        at += n as u64;
        progress(at, total);
        if at.saturating_sub(checkpoint_at) >= options.checkpoint_interval_bytes.max(1) {
            out.flush()
                .map_err(|source| Error::Io { offset: at, source })?;
            out.sync_all()
                .map_err(|source| Error::Io { offset: at, source })?;
            save_checkpoint(map_file, at, &hasher, &mut checkpoint_at)?;
        }
        if at % (256 * 1024 * 1024) < COPY_BLOCK as u64 {
            out.flush().map_err(|e| Error::Io {
                offset: at,
                source: e,
            })?;
        }
    }
    out.flush().map_err(|e| Error::Io {
        offset: at,
        source: e,
    })?;
    out.sync_all().map_err(|e| Error::Io {
        offset: at,
        source: e,
    })?;
    save_checkpoint(map_file, at, &hasher, &mut checkpoint_at)?;
    let digest = hasher.finish_hex();

    let mut verified = false;
    let mut verification = "not requested".to_string();
    if options.verify {
        let check = hash_reader(&mut out)?;
        if check != digest {
            warnings.push(
                "destination hash does not match the hash computed while copying; the image may \
                 be unreliable"
                    .to_string(),
            );
            verification = format!("FAILED: copy {digest}, re-read {check}");
        } else {
            verified = true;
            verification = "SHA-256 of the finished image matches the copy stream".to_string();
        }
    } else {
        warnings.push(
            "image verification disabled; the digest was computed while copying but was not \
             re-read from disk"
                .to_string(),
        );
    }
    drop(out);
    if options.verify && !verified {
        return Err(Error::Other(format!(
            "image verification failed; the unverified partial image remains at {}",
            partial.display()
        )));
    }
    if destination_root.entry_exists(&image_name)? {
        return Err(Error::UnsafeDestination(format!(
            "{} appeared while imaging; the partial image remains at {}",
            image_path.display(),
            partial.display()
        )));
    }
    destination_root
        .rename_file(&partial_name, &image_name)
        .map_err(|error| {
            Error::Other(format!(
                "image copied but could not be finalized at {}: {error}; partial image remains at {}",
                image_path.display(),
                partial.display()
            ))
        })?;

    let manifest = ImageManifest {
        schema: crate::SCHEMA_VERSION,
        tool: "relift".to_string(),
        tool_version: VERSION.to_string(),
        created: now_iso(),
        source: source_identity,
        image_path: image_path.to_string_lossy().to_string(),
        bytes: at,
        sha256: digest.clone(),
        verified,
        resumed,
        verification,
        warnings: warnings.clone(),
    };
    let manifest_path = write_manifest(&destination_root, &manifest_name, &image_path, &manifest)?;

    // The finished image and manifest are durable now, so the transient map
    // can be removed without losing the ability to resume an interrupted copy.
    drop(map_output.file.take());
    map_output.remove();

    Ok(ImageResult {
        path: image_path,
        bytes: at,
        sha256: digest,
        verified,
        resumed,
        manifest_path: Some(manifest_path),
        elapsed_seconds: started.elapsed().as_secs_f64(),
    })
}

/// Finish publishing an image if the process stopped after the `.partial`
/// file was renamed but before its manifest was written.
fn resume_finalized_image(
    destination_root: &DestinationDirectory,
    image_name: &Path,
    map_name: &Path,
    manifest_name: &Path,
    source: &dyn ReadOnlySource,
    total: u64,
) -> Result<ImageResult> {
    let started = std::time::Instant::now();
    let mut map_output = destination_root.open_existing_file(map_name)?;
    let map_file = map_output
        .file
        .as_mut()
        .ok_or_else(|| Error::Other("opened image map did not contain a file handle".into()))?;
    if source.is_same_file(map_file)? {
        return Err(Error::UnsafeDestination(
            "image checkpoint map is the same underlying file as the read-only source; refusing to resume"
                .into(),
        ));
    }
    let mut image_output = destination_root.open_existing_file(image_name)?;
    let image_file = image_output
        .file
        .as_mut()
        .ok_or_else(|| Error::Other("opened final image did not contain a file handle".into()))?;
    if files_are_same(map_file, image_file)? {
        return Err(Error::UnsafeDestination(
            "image checkpoint map and finalized image are the same underlying file; refusing to resume"
                .into(),
        ));
    }
    let source_identity = source.identity();
    let checkpoint = load_map(map_file, &source_identity, total)?;
    if checkpoint.offset != total {
        return Err(Error::Session(format!(
            "final image exists but its map reaches only {} of {total} bytes",
            checkpoint.offset
        )));
    }

    let image_len = image_file
        .metadata()
        .map_err(|source| Error::Io { offset: 0, source })?
        .len();
    if image_len != total {
        return Err(Error::Session(format!(
            "final image is {image_len} bytes, expected {total}; refusing to publish its manifest"
        )));
    }
    let digest = hash_reader(image_file)?;
    if digest != checkpoint.sha256 {
        return Err(Error::Session(
            "final image does not match its durable checkpoint; refusing to publish its manifest"
                .into(),
        ));
    }
    drop(image_output.file.take());

    let image_path = image_output.path.clone();
    let manifest = ImageManifest {
        schema: crate::SCHEMA_VERSION,
        tool: "relift".to_string(),
        tool_version: VERSION.to_string(),
        created: now_iso(),
        source: source_identity,
        image_path: image_path.to_string_lossy().to_string(),
        bytes: total,
        sha256: digest.clone(),
        verified: true,
        resumed: true,
        verification: "SHA-256 of the finished image matches the durable checkpoint".to_string(),
        warnings: Vec::new(),
    };
    let manifest_path = write_manifest(destination_root, manifest_name, &image_path, &manifest)?;
    drop(map_output.file.take());
    map_output.remove();

    Ok(ImageResult {
        path: image_path,
        bytes: total,
        sha256: digest,
        verified: true,
        resumed: true,
        manifest_path: Some(manifest_path),
        elapsed_seconds: started.elapsed().as_secs_f64(),
    })
}

fn write_manifest(
    destination_root: &DestinationDirectory,
    manifest_name: &Path,
    image_path: &Path,
    manifest: &ImageManifest,
) -> Result<PathBuf> {
    let manifest_path = manifest_path(image_path);
    let json = serde_json::to_vec_pretty(manifest)?;
    let mut manifest_output = destination_root.create_new_file(manifest_name)?;
    let mut manifest_file = manifest_output.file.take().ok_or_else(|| {
        Error::Other("new image manifest did not contain an open file handle".into())
    })?;
    let manifest_write = manifest_file
        .write_all(&json)
        .and_then(|()| manifest_file.flush())
        .and_then(|()| manifest_file.sync_all());
    drop(manifest_file);
    if let Err(source) = manifest_write {
        let _ = manifest_output.remove();
        return Err(Error::Io { offset: 0, source });
    }
    Ok(manifest_path)
}

fn manifest_path(dest: &Path) -> PathBuf {
    let name = dest.file_name().unwrap_or_default();
    dest.with_file_name(suffixed_name(Path::new(name), ".relift.json"))
}

fn write_map_header(file: &mut File, source: &crate::source::SourceIdentity) -> Result<()> {
    let record = ImageMapRecord::Header {
        schema: IMAGE_MAP_SCHEMA,
        source: source.clone(),
    };
    serde_json::to_writer(&mut *file, &record)?;
    file.write_all(b"\n")
        .map_err(|source| Error::Io { offset: 0, source })?;
    file.sync_all()
        .map_err(|source| Error::Io { offset: 0, source })
}

/// Read the append-only checkpoint log. A torn or malformed tail is discarded;
/// all earlier complete checkpoints remain usable.
fn load_map(
    file: &mut File,
    source: &crate::source::SourceIdentity,
    total: u64,
) -> Result<ImageCheckpoint> {
    file.seek(SeekFrom::Start(0))
        .map_err(|source| Error::Io { offset: 0, source })?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let read = reader
        .read_until(b'\n', &mut line)
        .map_err(|source| Error::Io { offset: 0, source })?;
    if read == 0 || line.last() != Some(&b'\n') {
        return Err(Error::Session(
            "image checkpoint map has no complete header".into(),
        ));
    }
    let header: ImageMapRecord = serde_json::from_slice(&line[..line.len() - 1])
        .map_err(|error| Error::Session(format!("invalid image checkpoint header: {error}")))?;
    match header {
        ImageMapRecord::Header {
            schema,
            source: recorded,
        } if schema == IMAGE_MAP_SCHEMA && recorded == *source => {}
        ImageMapRecord::Header { schema, .. } if schema != IMAGE_MAP_SCHEMA => {
            return Err(Error::Session(format!(
                "image checkpoint schema {schema} is not supported (expected {IMAGE_MAP_SCHEMA})"
            )))
        }
        ImageMapRecord::Header { .. } => {
            return Err(Error::SourceMismatch(
                "image checkpoint source identity does not match the selected source".into(),
            ))
        }
        ImageMapRecord::Checkpoint { .. } => {
            return Err(Error::Session(
                "image checkpoint map does not begin with a header".into(),
            ))
        }
    }

    let mut valid_len = read as u64;
    let mut checkpoint = ImageCheckpoint {
        offset: 0,
        sha256: Hasher::new().snapshot_hex(),
    };
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|source| Error::Io {
                offset: valid_len,
                source,
            })?;
        if read == 0 || line.last() != Some(&b'\n') {
            break;
        }
        let Ok(ImageMapRecord::Checkpoint { offset, sha256 }) =
            serde_json::from_slice::<ImageMapRecord>(&line[..line.len() - 1])
        else {
            break;
        };
        if offset < checkpoint.offset || offset > total || !valid_sha256(&sha256) {
            break;
        }
        checkpoint = ImageCheckpoint { offset, sha256 };
        valid_len = valid_len.saturating_add(read as u64);
    }

    let file = reader.into_inner();
    file.set_len(valid_len).map_err(|source| Error::Io {
        offset: valid_len,
        source,
    })?;
    file.seek(SeekFrom::End(0)).map_err(|source| Error::Io {
        offset: valid_len,
        source,
    })?;
    Ok(checkpoint)
}

fn save_checkpoint(
    file: &mut File,
    offset: u64,
    hasher: &Hasher,
    previous_offset: &mut u64,
) -> Result<()> {
    if offset == *previous_offset {
        return Ok(());
    }
    let record = ImageMapRecord::Checkpoint {
        offset,
        sha256: hasher.snapshot_hex(),
    };
    serde_json::to_writer(&mut *file, &record)?;
    file.write_all(b"\n")
        .map_err(|source| Error::Io { offset, source })?;
    file.sync_all()
        .map_err(|source| Error::Io { offset, source })?;
    *previous_offset = offset;
    Ok(())
}

fn hash_prefix(file: &mut File, len: u64) -> Result<Hasher> {
    file.seek(SeekFrom::Start(0))
        .map_err(|source| Error::Io { offset: 0, source })?;
    let mut hasher = Hasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut at = 0u64;
    while at < len {
        let want = (len - at).min(buf.len() as u64) as usize;
        let n = file
            .read(&mut buf[..want])
            .map_err(|source| Error::Io { offset: at, source })?;
        if n == 0 {
            return Err(Error::UnexpectedEof {
                offset: at,
                wanted: want,
                got: 0,
            });
        }
        hasher.update(&buf[..n]);
        at += n as u64;
    }
    file.seek(SeekFrom::Start(len))
        .map_err(|source| Error::Io {
            offset: len,
            source,
        })?;
    Ok(hasher)
}

fn hash_source_prefix(source: &dyn ReadOnlySource, len: u64) -> Result<Hasher> {
    let mut hasher = Hasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut at = 0u64;
    while at < len {
        let want = (len - at).min(buf.len() as u64) as usize;
        source.read_exact_at(at, &mut buf[..want])?;
        hasher.update(&buf[..want]);
        at += want as u64;
    }
    Ok(hasher)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn suffixed_name(name: &Path, suffix: &str) -> PathBuf {
    let mut result = name.as_os_str().to_os_string();
    result.push(suffix);
    PathBuf::from(result)
}

/// Refuse to image a raw device onto a filesystem that might be on that same
/// disk. This is a separate helper so the CLI can present a focused message.
pub fn check_raw_image_destination(
    source: &crate::source::SourceIdentity,
    dest: &Path,
) -> Result<()> {
    if source.kind == SourceKind::RawDevice {
        let parent = dest.parent().unwrap_or_else(|| Path::new("."));
        safety::validate_destination(source, parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileSource;
    use std::fs;

    #[test]
    fn image_roundtrip_and_manifest() {
        let dir = std::env::temp_dir().join(format!("relift-image-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("src.img");
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        fs::write(&src_path, &data).unwrap();
        let src = FileSource::open(&src_path).unwrap();
        let dest = dir.join("copy.img");
        let mut progress = |_a: u64, _b: u64| {};
        let result = create_image(&src, &dest, &ImageOptions::default(), &mut progress).unwrap();
        assert_eq!(result.bytes, data.len() as u64);
        assert!(result.verified);
        assert_eq!(fs::read(&dest).unwrap(), data);
        let manifest = fs::read_to_string(manifest_path(&dest)).unwrap();
        assert!(manifest.contains("sha256"));
        assert!(create_image(&src, &dest, &ImageOptions::default(), &mut progress).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_manifest_is_never_overwritten() {
        let dir = std::env::temp_dir().join(format!(
            "relift-image-manifest-collision-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("source.img");
        fs::write(&src_path, b"source bytes").unwrap();
        let source = FileSource::open(&src_path).unwrap();
        let destination = dir.join("copy.img");
        let manifest = manifest_path(&destination);
        fs::write(&manifest, b"keep this file").unwrap();
        let mut progress = |_current: u64, _total: u64| {};

        let result = create_image(
            &source,
            &destination,
            &ImageOptions::default(),
            &mut progress,
        );

        assert!(result.is_err());
        assert!(!destination.exists());
        assert_eq!(fs::read(&manifest).unwrap(), b"keep this file");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn final_image_name_collision_does_not_replace_existing_file() {
        let dir = std::env::temp_dir().join(format!(
            "relift-image-final-collision-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("source.img");
        fs::write(&src_path, b"source bytes").unwrap();
        let source = FileSource::open(&src_path).unwrap();
        let root = DestinationDirectory::open(&dir.canonicalize().unwrap()).unwrap();
        let partial_name = PathBuf::from("copy.img.partial");
        let final_name = PathBuf::from("copy.img");
        let mut partial = root.create_new_file(&partial_name).unwrap();
        partial
            .file
            .as_mut()
            .unwrap()
            .write_all(b"new image")
            .unwrap();
        drop(partial);
        fs::write(dir.join(&final_name), b"existing image").unwrap();
        let mut progress = |_current: u64, _total: u64| {};

        let result = create_image(
            &source,
            &dir.join(&final_name),
            &ImageOptions::default(),
            &mut progress,
        );

        assert!(result.is_err());
        assert_eq!(fs::read(dir.join(&final_name)).unwrap(), b"existing image");
        let _ = fs::remove_dir_all(&dir);
    }

    fn interrupted_image(dir: &Path, source_path: &Path) -> (Vec<u8>, ImageOptions) {
        fs::create_dir_all(dir).unwrap();
        let data: Vec<u8> = (0..10 * 1024 * 1024)
            .map(|index| (index % 251) as u8)
            .collect();
        fs::write(source_path, &data).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let options = ImageOptions {
            checkpoint_interval_bytes: 4 * 1024 * 1024,
            cancelled,
            ..ImageOptions::default()
        };
        let source = FileSource::open(source_path).unwrap();
        let mut progress = |done: u64, _total: u64| {
            if done >= COPY_BLOCK as u64 {
                options.cancelled.store(true, Ordering::Relaxed);
            }
        };
        let error =
            create_image(&source, &dir.join("copy.img"), &options, &mut progress).unwrap_err();
        assert!(matches!(error, Error::Cancelled));
        (data, options)
    }

    #[test]
    fn interrupted_image_resumes_after_a_torn_map_tail() {
        let dir = std::env::temp_dir().join(format!("relift-image-resume-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let source_path = dir.join("source.img");
        let (data, options) = interrupted_image(&dir, &source_path);
        let map = dir.join("copy.img.relift-map.jsonl");
        use std::io::Write as _;
        fs::OpenOptions::new()
            .append(true)
            .open(&map)
            .unwrap()
            .write_all(b"{\"record\":\"checkpoint\"")
            .unwrap();

        let source = FileSource::open(&source_path).unwrap();
        let options = ImageOptions {
            resume: true,
            cancelled: Arc::new(AtomicBool::new(false)),
            ..options
        };
        let mut progress = |_done: u64, _total: u64| {};
        let result = create_image(&source, &dir.join("copy.img"), &options, &mut progress).unwrap();

        assert!(result.resumed);
        assert!(result.verified);
        assert_eq!(result.bytes, data.len() as u64);
        assert_eq!(fs::read(&result.path).unwrap(), data);
        assert!(!dir.join("copy.img.partial").exists());
        assert!(!map.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_resume_refuses_a_changed_source() {
        let dir = std::env::temp_dir().join(format!(
            "relift-image-source-mismatch-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let source_path = dir.join("source.img");
        let (_data, options) = interrupted_image(&dir, &source_path);
        let mut changed = fs::read(&source_path).unwrap();
        changed[0] ^= 0xff;
        fs::write(&source_path, changed).unwrap();

        let source = FileSource::open(&source_path).unwrap();
        let options = ImageOptions {
            resume: true,
            cancelled: Arc::new(AtomicBool::new(false)),
            ..options
        };
        let mut progress = |_done: u64, _total: u64| {};
        let result = create_image(&source, &dir.join("copy.img"), &options, &mut progress);
        assert!(matches!(result, Err(Error::SourceMismatch(_))));
        assert!(dir.join("copy.img.partial").exists());
        assert!(dir.join("copy.img.relift-map.jsonl").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_resume_detects_changes_in_unsampled_source_regions() {
        let dir = std::env::temp_dir().join(format!(
            "relift-image-unsampled-source-change-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let source_path = dir.join("source.img");
        let partial_path = dir.join("copy.img.partial");
        let map_path = dir.join("copy.img.relift-map.jsonl");
        let source_len = 512 * 1024 * 1024;
        let checkpoint_offset = 128 * 1024 * 1024;
        // Block 999 is skipped by the quadratic identity-sampling schedule for
        // this 512 MiB source, but it lies inside the saved image prefix.
        let changed_block = 999u64;
        let changed_offset = changed_block * 64 * 1024 + 7;

        let source_file = fs::File::create(&source_path).unwrap();
        source_file.set_len(source_len).unwrap();
        drop(source_file);
        let source = FileSource::open(&source_path).unwrap();
        let mut source_file = fs::OpenOptions::new()
            .write(true)
            .open(&source_path)
            .unwrap();
        source_file.seek(SeekFrom::Start(changed_offset)).unwrap();
        source_file.write_all(&[0xA5]).unwrap();
        drop(source_file);

        let mut partial_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&partial_path)
            .unwrap();
        partial_file.set_len(checkpoint_offset).unwrap();
        let hasher = hash_prefix(&mut partial_file, checkpoint_offset).unwrap();
        drop(partial_file);

        let root = DestinationDirectory::open(&dir.canonicalize().unwrap()).unwrap();
        let mut map = root
            .create_new_file(Path::new("copy.img.relift-map.jsonl"))
            .unwrap();
        let map_file = map.file.as_mut().unwrap();
        write_map_header(map_file, &source.identity()).unwrap();
        let mut checkpoint_at = 0;
        save_checkpoint(map_file, checkpoint_offset, &hasher, &mut checkpoint_at).unwrap();
        drop(map);

        let options = ImageOptions {
            resume: true,
            ..ImageOptions::default()
        };
        let mut progress = |_done: u64, _total: u64| {};
        let result = create_image(&source, &dir.join("copy.img"), &options, &mut progress);

        assert!(matches!(result, Err(Error::SourceMismatch(_))));
        assert_eq!(
            fs::metadata(&partial_path).unwrap().len(),
            checkpoint_offset
        );
        assert!(map_path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_resume_refuses_a_modified_partial_prefix() {
        let dir =
            std::env::temp_dir().join(format!("relift-image-map-mismatch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let source_path = dir.join("source.img");
        let (_data, options) = interrupted_image(&dir, &source_path);
        let partial = dir.join("copy.img.partial");
        let mut partial_bytes = fs::read(&partial).unwrap();
        partial_bytes[0] ^= 0xff;
        fs::write(&partial, partial_bytes).unwrap();

        let source = FileSource::open(&source_path).unwrap();
        let options = ImageOptions {
            resume: true,
            cancelled: Arc::new(AtomicBool::new(false)),
            ..options
        };
        let mut progress = |_done: u64, _total: u64| {};
        let result = create_image(&source, &dir.join("copy.img"), &options, &mut progress);
        assert!(matches!(result, Err(Error::Session(_))));
        assert!(partial.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_resume_refuses_a_partial_hard_link_to_the_source() {
        let dir = std::env::temp_dir().join(format!(
            "relift-image-source-hardlink-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let source_path = dir.join("source.img");
        let partial_path = dir.join("copy.img.partial");
        let map_path = dir.join("copy.img.relift-map.jsonl");
        fs::write(&source_path, b"read-only source bytes").unwrap();
        fs::hard_link(&source_path, &partial_path).unwrap();
        let source = FileSource::open(&source_path).unwrap();
        let root = DestinationDirectory::open(&dir.canonicalize().unwrap()).unwrap();
        let mut map = root
            .create_new_file(Path::new("copy.img.relift-map.jsonl"))
            .unwrap();
        write_map_header(map.file.as_mut().unwrap(), &source.identity()).unwrap();
        drop(map);
        let original = fs::read(&source_path).unwrap();

        let options = ImageOptions {
            resume: true,
            ..ImageOptions::default()
        };
        let mut progress = |_done: u64, _total: u64| {};
        let result = create_image(&source, &dir.join("copy.img"), &options, &mut progress);

        assert!(matches!(result, Err(Error::UnsafeDestination(_))));
        assert_eq!(fs::read(&source_path).unwrap(), original);
        assert_eq!(fs::read(&partial_path).unwrap(), original);
        assert!(map_path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_resume_refuses_a_checkpoint_map_hard_link_to_the_source() {
        let dir = std::env::temp_dir().join(format!(
            "relift-image-map-source-hardlink-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let source_path = dir.join("source.img");
        let partial_path = dir.join("copy.img.partial");
        let map_path = dir.join("copy.img.relift-map.jsonl");
        let source_bytes = b"read-only source bytes";
        fs::write(&source_path, source_bytes).unwrap();
        fs::write(&partial_path, b"partial image").unwrap();
        fs::hard_link(&source_path, &map_path).unwrap();
        let source = FileSource::open(&source_path).unwrap();

        let options = ImageOptions {
            resume: true,
            ..ImageOptions::default()
        };
        let mut progress = |_done: u64, _total: u64| {};
        let result = create_image(&source, &dir.join("copy.img"), &options, &mut progress);

        assert!(matches!(result, Err(Error::UnsafeDestination(_))));
        assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
        assert_eq!(fs::read(&map_path).unwrap(), source_bytes);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_resume_refuses_a_checkpoint_map_hard_link_to_the_partial() {
        let dir = std::env::temp_dir().join(format!(
            "relift-image-map-partial-hardlink-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let source_path = dir.join("source.img");
        let partial_path = dir.join("copy.img.partial");
        let map_path = dir.join("copy.img.relift-map.jsonl");
        fs::write(&source_path, b"read-only source bytes").unwrap();
        let source = FileSource::open(&source_path).unwrap();
        let root = DestinationDirectory::open(&dir.canonicalize().unwrap()).unwrap();
        let mut map = root
            .create_new_file(Path::new("copy.img.relift-map.jsonl"))
            .unwrap();
        write_map_header(map.file.as_mut().unwrap(), &source.identity()).unwrap();
        drop(map);
        fs::hard_link(&map_path, &partial_path).unwrap();
        let original_map = fs::read(&map_path).unwrap();

        let options = ImageOptions {
            resume: true,
            ..ImageOptions::default()
        };
        let mut progress = |_done: u64, _total: u64| {};
        let result = create_image(&source, &dir.join("copy.img"), &options, &mut progress);

        assert!(matches!(result, Err(Error::UnsafeDestination(_))));
        assert_eq!(fs::read(&map_path).unwrap(), original_map);
        assert_eq!(fs::read(&partial_path).unwrap(), original_map);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_resume_publishes_manifest_after_final_rename_interruption() {
        let dir = std::env::temp_dir().join(format!(
            "relift-image-finalize-resume-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let source_path = dir.join("source.img");
        let image_path = dir.join("copy.img");
        let data: Vec<u8> = (0..2 * 1024 * 1024)
            .map(|index| (index % 239) as u8)
            .collect();
        fs::write(&source_path, &data).unwrap();
        let source = FileSource::open(&source_path).unwrap();
        let root = DestinationDirectory::open(&dir.canonicalize().unwrap()).unwrap();
        let partial_name = PathBuf::from("copy.img.partial");
        let map_name = PathBuf::from("copy.img.relift-map.jsonl");
        let mut partial = root.create_new_file(&partial_name).unwrap();
        partial.file.as_mut().unwrap().write_all(&data).unwrap();
        partial.file.as_mut().unwrap().sync_all().unwrap();
        drop(partial);
        let mut map = root.create_new_file(&map_name).unwrap();
        let map_file = map.file.as_mut().unwrap();
        write_map_header(map_file, &source.identity()).unwrap();
        let mut hasher = Hasher::new();
        hasher.update(&data);
        let mut checkpoint_at = 0;
        save_checkpoint(map_file, data.len() as u64, &hasher, &mut checkpoint_at).unwrap();
        drop(map);

        // Model a process stopping after publication of the image but before
        // creation of the manifest.
        fs::rename(dir.join(partial_name), &image_path).unwrap();
        let options = ImageOptions {
            resume: true,
            ..ImageOptions::default()
        };
        let mut progress = |_done: u64, _total: u64| {};
        let result = create_image(&source, &image_path, &options, &mut progress).unwrap();

        assert!(result.resumed);
        assert!(result.verified);
        assert_eq!(fs::read(&result.path).unwrap(), data);
        assert!(result.manifest_path.unwrap().exists());
        assert!(!dir.join(map_name).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
