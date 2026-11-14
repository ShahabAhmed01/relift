//! Read-only imaging of a source to a new file.
//!
//! The destination is opened with `create_new`; an existing file is never
//! overwritten. The image is written to `<dest>.partial` and renamed only after
//! a successful flush. With verification enabled (the default in the CLI) the
//! finished image is re-read and hashed, and a sidecar manifest records both the
//! source identity and the image digest.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::hash::Hasher;
use crate::safety;
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
    safety::validate_destination(&src.identity(), parent)?;
    if dest.exists() {
        return Err(Error::UnsafeDestination(format!(
            "{} already exists; refusing to overwrite it",
            dest.display()
        )));
    }
    let partial = partial_path(dest);
    if partial.exists() {
        return Err(Error::UnsafeDestination(format!(
            "{} already exists; remove it or choose another image name",
            partial.display()
        )));
    }
    let started = std::time::Instant::now();
    let total = src.len();
    let mut out = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&partial)
        .map_err(|e| Error::Io {
            offset: 0,
            source: e,
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
    drop(out);
    let digest = hasher.finish_hex();

    let mut verified = false;
    let mut verification = "not requested".to_string();
    if options.verify {
        let mut f = File::open(&partial).map_err(|e| Error::Io {
            offset: 0,
            source: e,
        })?;
        let mut h = Hasher::new();
        let mut vbuf = vec![0u8; COPY_BLOCK.max(1)];
        loop {
            let n = f.read(&mut vbuf).map_err(|e| Error::Io {
                offset: 0,
                source: e,
            })?;
            if n == 0 {
                break;
            }
            h.update(&vbuf[..n]);
        }
        let check = h.finish_hex();
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

    fs::rename(&partial, dest).map_err(|e| {
        Error::Other(format!(
            "image copied but could not be renamed from {} to {}: {e}",
            partial.display(),
            dest.display()
        ))
    })?;

    let manifest = ImageManifest {
        schema: crate::SCHEMA_VERSION,
        tool: "relift".to_string(),
        tool_version: VERSION.to_string(),
        created: now_iso(),
        source: src.identity(),
        image_path: dest.to_string_lossy().to_string(),
        bytes: at,
        sha256: digest.clone(),
        verified,
        verification,
        warnings: warnings.clone(),
    };
    let manifest_path = manifest_path(dest);
    let json = serde_json::to_vec_pretty(&manifest)?;
    fs::write(&manifest_path, json).map_err(|e| Error::Io {
        offset: 0,
        source: e,
    })?;

    if !warnings.is_empty() && options.verify && !verified {
        return Err(Error::Other(format!(
            "image written to {} but verification failed; see {}",
            dest.display(),
            manifest_path.display()
        )));
    }
    Ok(ImageResult {
        path: dest.to_path_buf(),
        bytes: at,
        sha256: digest,
        verified,
        manifest_path: Some(manifest_path),
        elapsed_seconds: started.elapsed().as_secs_f64(),
    })
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".partial");
    PathBuf::from(s)
}

fn manifest_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".relift.json");
    PathBuf::from(s)
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
}
