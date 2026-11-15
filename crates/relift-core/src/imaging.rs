//! Read-only imaging of a source to a new file.
//!
//! The destination is opened with `create_new`; an existing file is never
//! overwritten. The image is written to `<dest>.partial` and renamed only after
//! a successful flush. With verification enabled (the default in the CLI) the
//! finished image is re-read and hashed, and a sidecar manifest records both the
//! source identity and the image digest.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::hash::{hash_reader, Hasher};
use crate::safety::{self, DestinationDirectory};
use crate::session::now_iso;
use crate::source::{ReadOnlySource, SourceKind};
use crate::VERSION;

const COPY_BLOCK: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ImageOptions {
    pub verify: bool,
    pub cancelled: Arc<AtomicBool>,
}

impl Default for ImageOptions {
    fn default() -> Self {
        Self {
            verify: true,
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
    pub verification: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageResult {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub verified: bool,
    pub manifest_path: Option<PathBuf>,
    pub elapsed_seconds: f64,
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
    let manifest_name = suffixed_name(&image_name, ".relift.json");
    for name in [&image_name, &partial_name, &manifest_name] {
        if destination_root.entry_exists(name)? {
            return Err(Error::UnsafeDestination(format!(
                "{} already exists; refusing to overwrite it",
                canonical_parent.join(name).display()
            )));
        }
    }
    let image_path = canonical_parent.join(&image_name);
    let partial = canonical_parent.join(&partial_name);
    let started = std::time::Instant::now();
    let total = src.len();
    let mut partial_output = destination_root.create_new_file(&partial_name)?;
    let mut out = partial_output.file.take().ok_or_else(|| {
        Error::Other("new partial image did not contain an open file handle".into())
    })?;
    let mut hasher = Hasher::new();
    let mut buf = vec![0u8; COPY_BLOCK];
    let mut at = 0u64;
    let mut warnings = Vec::new();
    while at < total {
        if options.cancelled.load(Ordering::Relaxed) {
            let _ = out.flush();
            let _ = out.sync_all();
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
        source: src.identity(),
        image_path: image_path.to_string_lossy().to_string(),
        bytes: at,
        sha256: digest.clone(),
        verified,
        verification,
        warnings: warnings.clone(),
    };
    let manifest_path = manifest_path(&image_path);
    let json = serde_json::to_vec_pretty(&manifest)?;
    let mut manifest_output = destination_root.create_new_file(&manifest_name)?;
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

    Ok(ImageResult {
        path: image_path,
        bytes: at,
        sha256: digest,
        verified,
        manifest_path: Some(manifest_path),
        elapsed_seconds: started.elapsed().as_secs_f64(),
    })
}

fn manifest_path(dest: &Path) -> PathBuf {
    let name = dest.file_name().unwrap_or_default();
    dest.with_file_name(suffixed_name(Path::new(name), ".relift.json"))
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
}
