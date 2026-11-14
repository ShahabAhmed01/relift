//! Read-only source abstraction.
//!
//! `ReadOnlySource` deliberately has no write method. The only way to create a
//! source is through [`FileSource::open`] (read-only `File::open`) or
//! [`RawDeviceSource::open`] (OS read-only handle). Recovery modules receive a
//! `&dyn ReadOnlySource` and therefore cannot write to the evidence.

use std::fmt;
use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Maximum size of a single `read_vec` allocation. Parsers that need more must
/// stream; this bound prevents a corrupt length field from exhausting memory.
pub const MAX_BUFFERED_READ: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Image,
    RawDevice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub kind: SourceKind,
    /// Path or device path exactly as the user supplied it.
    pub path: String,
    /// Stable OS device identifier when available (physical drive number, etc.).
    pub device_id: Option<String>,
    pub size: u64,
    /// SHA-256 over the size and sampled head/tail bytes. Used to refuse a
    /// resume against a different image that happens to reuse the same path.
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct SourceDescription {
    pub identity: SourceIdentity,
    pub sector_size: u32,
    pub read_only: bool,
    pub warnings: Vec<String>,
}

/// A positional, read-only byte source.
pub trait ReadOnlySource: Send + Sync {
    fn len(&self) -> u64;
    fn sector_size(&self) -> u32 {
        512
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize>;
    fn identity(&self) -> SourceIdentity;
    fn description(&self) -> SourceDescription {
        SourceDescription {
            identity: self.identity(),
            sector_size: self.sector_size(),
            read_only: true,
            warnings: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read exactly `buf.len()` bytes or fail.
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        let mut done = 0usize;
        while done < buf.len() {
            let at = offset
                .checked_add(done as u64)
                .ok_or_else(|| Error::Corrupt("read offset overflow".into()))?;
            match self.read_at(at, &mut buf[done..])? {
                0 => {
                    return Err(Error::UnexpectedEof {
                        offset: at,
                        wanted: buf.len() - done,
                        got: 0,
                    })
                }
                n => done += n,
            }
        }
        Ok(())
    }

    /// Read a bounded region into memory.
    fn read_vec(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        if len > MAX_BUFFERED_READ {
            return Err(Error::Corrupt(format!(
                "refusing to buffer {len} bytes (limit {MAX_BUFFERED_READ})"
            )));
        }
        let mut buf = vec![0u8; len as usize];
        self.read_exact_at(offset, &mut buf)?;
        Ok(buf)
    }
}

/// A disk image or any other regular file, opened with `File::open` (read-only).
#[derive(Debug)]
pub struct FileSource {
    file: File,
    path: PathBuf,
    len: u64,
    identity: SourceIdentity,
}

impl FileSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| Error::Io {
            offset: 0,
            source: e,
        })?;
        let meta = file.metadata().map_err(|e| Error::Io {
            offset: 0,
            source: e,
        })?;
        if !meta.is_file() {
            return Err(Error::invalid(format!(
                "{} is not a regular file; use a raw-device path for devices",
                path.display()
            )));
        }
        let len = meta.len();
        let identity = compute_identity(SourceKind::Image, path, len, None);
        Ok(Self {
            file,
            path: path.to_path_buf(),
            len,
            identity,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ReadOnlySource for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        if offset >= self.len {
            return Ok(0);
        }
        let remaining = (self.len - offset).min(buf.len() as u64) as usize;
        let mut read = 0usize;
        while read < remaining {
            let n = read_file_at(&self.file, offset + read as u64, &mut buf[read..remaining])
                .map_err(|e| Error::io(offset + read as u64, e))?;
            if n == 0 {
                break;
            }
            read += n;
        }
        Ok(read)
    }

    fn identity(&self) -> SourceIdentity {
        self.identity.clone()
    }
}

#[cfg(unix)]
fn read_file_at(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buf, offset)
}

#[cfg(windows)]
fn read_file_at(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buf, offset)
}

/// A raw physical disk or volume handle.
///
/// On Windows the handle is opened with `GENERIC_READ` only. `FILE_SHARE_READ |
/// FILE_SHARE_WRITE` is requested because Windows refuses to open a mounted
/// volume exclusively without dismounting it; requesting share access does not
/// grant Relift write permission. On Unix the file is opened with `File::open`.
pub struct RawDeviceSource {
    inner: raw::RawInner,
    path: String,
    len: u64,
    identity: SourceIdentity,
}

impl fmt::Debug for RawDeviceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawDeviceSource")
            .field("path", &self.path)
            .field("len", &self.len)
            .finish()
    }
}

impl RawDeviceSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let p = path.as_ref();
        let path_str = p.to_string_lossy().to_string();
        let (inner, len, device_id) = raw::open(p)?;
        let identity = compute_identity(SourceKind::RawDevice, p, len, device_id.clone());
        Ok(Self {
            inner,
            path: path_str,
            len,
            identity,
        })
    }
}

impl ReadOnlySource for RawDeviceSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn sector_size(&self) -> u32 {
        self.inner.sector_size()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        self.inner
            .read_at(offset, buf)
            .map_err(|e| Error::io(offset, e))
    }

    fn identity(&self) -> SourceIdentity {
        self.identity.clone()
    }

    fn description(&self) -> SourceDescription {
        let mut warnings = Vec::new();
        if self.identity.kind == SourceKind::RawDevice {
            warnings.push(
                "Raw device access is read-only in Relift, but a mounted filesystem may still \
                 write in the background. Prefer imaging the device and scanning the image."
                    .to_string(),
            );
        }
        SourceDescription {
            identity: self.identity.clone(),
            sector_size: self.sector_size(),
            read_only: true,
            warnings,
        }
    }
}

/// Open either a regular image file or a raw device. Windows paths beginning
/// with `\\.\` or `\\?\` and Unix paths beginning with `/dev/` are treated as
/// devices; everything else is opened as a read-only file.
pub fn open_source(path: impl AsRef<Path>) -> Result<Box<dyn ReadOnlySource>> {
    let p = path.as_ref();
    if looks_like_device(p) {
        Ok(Box::new(RawDeviceSource::open(p)?))
    } else {
        Ok(Box::new(FileSource::open(p)?))
    }
}

pub fn looks_like_device(path: &Path) -> bool {
    let s = path.to_string_lossy();
    #[cfg(windows)]
    {
        s.starts_with(r"\\.\") || s.starts_with(r"\\?\") || s.starts_with(r"\??\")
    }
    #[cfg(not(windows))]
    {
        s.starts_with("/dev/")
    }
}

fn compute_identity(
    kind: SourceKind,
    path: &Path,
    size: u64,
    device_id: Option<String>,
) -> SourceIdentity {
    let mut hasher = Sha256::new();
    hasher.update(b"relift-source-identity-v1");
    hasher.update(size.to_le_bytes());
    if let Some(id) = &device_id {
        hasher.update(id.as_bytes());
    }
    // Sample the head and tail. For small files this hashes everything, which
    // makes fixture and image identity checks exact.
    let sample = 64 * 1024u64;
    let mut hit_any = false;
    if let Ok(f) = File::open(path) {
        if let Ok(meta) = f.metadata() {
            if meta.is_file() {
                if let Ok(bytes) = read_sample(&f, 0, sample.min(size)) {
                    hasher.update(&bytes);
                    hit_any = true;
                }
                if size > sample {
                    if let Ok(bytes) = read_sample(&f, size - sample.min(size), sample) {
                        hasher.update(&bytes);
                        hit_any = true;
                    }
                }
            }
        }
    }
    if !hit_any {
        hasher.update(b"unreadable-sample");
        hasher.update(path.to_string_lossy().as_bytes());
    }
    let fingerprint = hex(&hasher.finalize());
    SourceIdentity {
        kind,
        path: path.to_string_lossy().to_string(),
        device_id,
        size,
        fingerprint,
    }
}

fn read_sample(file: &File, offset: u64, len: u64) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len as usize];
    let mut done = 0usize;
    while done < buf.len() {
        let n = read_file_at(file, offset + done as u64, &mut buf[done..])?;
        if n == 0 {
            break;
        }
        done += n;
    }
    buf.truncate(done);
    Ok(buf)
}

pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(windows)]
mod raw {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    type Handle = *mut c_void;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const GENERIC_READ: u32 = 0x8000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
    const ERROR_IO_PENDING: u32 = 997;
    const IOCTL_DISK_GET_LENGTH_INFO: u32 = 0x0007_405C;
    const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x002D_1080;
    const IOCTL_DISK_GET_DRIVE_GEOMETRY_EX: u32 = 0x0007_00A0;

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: *mut c_void,
    }

    impl Overlapped {
        fn at(offset: u64) -> Self {
            Self {
                internal: 0,
                internal_high: 0,
                offset: offset as u32,
                offset_high: (offset >> 32) as u32,
                event: std::ptr::null_mut(),
            }
        }
    }

    #[repr(C)]
    #[derive(Default)]
    struct DeviceNumber {
        device_type: u32,
        device_number: u32,
        partition_number: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct DiskGeometry {
        cylinders: i64,
        media_type: u32,
        tracks_per_cylinder: u32,
        sectors_per_track: u32,
        bytes_per_sector: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct DiskGeometryEx {
        geometry: DiskGeometry,
        disk_size: i64,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
        fn ReadFile(
            file: Handle,
            buffer: *mut u8,
            bytes_to_read: u32,
            bytes_read: *mut u32,
            overlapped: *mut Overlapped,
        ) -> i32;
        fn GetOverlappedResult(
            file: Handle,
            overlapped: *mut Overlapped,
            bytes_transferred: *mut u32,
            wait: i32,
        ) -> i32;
        fn CloseHandle(object: Handle) -> i32;
        fn DeviceIoControl(
            device: Handle,
            control_code: u32,
            in_buffer: *mut c_void,
            in_size: u32,
            out_buffer: *mut c_void,
            out_size: u32,
            bytes_returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn GetLastError() -> u32;
    }

    pub struct RawInner {
        handle: Handle,
        sector_size: u32,
    }

    // SAFETY: the handle refers to a kernel object that is safe to use from any
    // thread for positional (OVERLAPPED) reads. All mutable state is on the
    // stack per call; the handle itself is never mutated after open.
    unsafe impl Send for RawInner {}
    // SAFETY: see above. Concurrent positional reads are supported by the OS.
    unsafe impl Sync for RawInner {}

    impl Drop for RawInner {
        fn drop(&mut self) {
            if self.handle != INVALID_HANDLE_VALUE {
                // SAFETY: handle was returned by CreateFileW and is valid.
                unsafe {
                    CloseHandle(self.handle);
                }
            }
        }
    }

    fn last_error(context: &str) -> io::Error {
        let code = unsafe { GetLastError() };
        io::Error::new(
            io::Error::from_raw_os_error(code as i32).kind(),
            format!("{context}: {}", io::Error::from_raw_os_error(code as i32)),
        )
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn open(path: &Path) -> crate::error::Result<(RawInner, u64, Option<String>)> {
        let w = wide(path);
        // SAFETY: `w` is NUL-terminated and valid for the duration of the call.
        let handle = unsafe {
            CreateFileW(
                w.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let e = last_error("open raw device read-only");
            return Err(crate::error::Error::Io {
                offset: 0,
                source: e,
            });
        }

        let mut len = 0i64;
        let mut returned = 0u32;
        // SAFETY: the output buffer is a valid i64 and the handle is open.
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_LENGTH_INFO,
                std::ptr::null_mut(),
                0,
                &mut len as *mut i64 as *mut c_void,
                std::mem::size_of::<i64>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        let mut sector_size = 512u32;
        let mut geom = DiskGeometryEx::default();
        // SAFETY: `geom` is a correctly sized output buffer for this IOCTL.
        let geom_ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
                std::ptr::null_mut(),
                0,
                &mut geom as *mut DiskGeometryEx as *mut c_void,
                std::mem::size_of::<DiskGeometryEx>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if geom_ok != 0 {
            if geom.geometry.bytes_per_sector >= 512 {
                sector_size = geom.geometry.bytes_per_sector;
            }
            if len <= 0 || ok == 0 {
                len = geom.disk_size;
            }
        }
        if len <= 0 || ok == 0 {
            // SAFETY: handle is valid.
            unsafe {
                CloseHandle(handle);
            }
            return Err(crate::error::Error::invalid(
                "could not determine device length; is this a valid raw device path?",
            ));
        }

        let mut dev = DeviceNumber::default();
        // SAFETY: `dev` is a valid output buffer for this IOCTL.
        let dev_ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                std::ptr::null_mut(),
                0,
                &mut dev as *mut DeviceNumber as *mut c_void,
                std::mem::size_of::<DeviceNumber>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        let device_id = if dev_ok != 0 {
            Some(format!(
                "disk:{}:part:{}:type:{}",
                dev.device_number, dev.partition_number, dev.device_type
            ))
        } else {
            None
        };

        Ok((
            RawInner {
                handle,
                sector_size,
            },
            len as u64,
            device_id,
        ))
    }

    impl RawInner {
        pub fn sector_size(&self) -> u32 {
            self.sector_size
        }

        pub fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let mut total = 0usize;
            while total < buf.len() {
                let at = offset + total as u64;
                let chunk = (buf.len() - total).min(1 << 20) as u32;
                let mut ov = Overlapped::at(at);
                let mut transferred = 0u32;
                // SAFETY: the handle is valid, the buffer is writable for
                // `chunk` bytes, and `ov` is a valid OVERLAPPED on the stack.
                let ok = unsafe {
                    ReadFile(
                        self.handle,
                        buf[total..].as_mut_ptr(),
                        chunk,
                        std::ptr::null_mut(),
                        &mut ov,
                    )
                };
                if ok == 0 {
                    let code = unsafe { GetLastError() };
                    if code != ERROR_IO_PENDING {
                        return Err(io::Error::from_raw_os_error(code as i32));
                    }
                }
                // SAFETY: `ov` belongs to the pending I/O and the handle is valid.
                let wait =
                    unsafe { GetOverlappedResult(self.handle, &mut ov, &mut transferred, 1) };
                if wait == 0 {
                    let code = unsafe { GetLastError() };
                    return Err(io::Error::from_raw_os_error(code as i32));
                }
                if transferred == 0 {
                    break;
                }
                total += transferred as usize;
            }
            Ok(total)
        }
    }
}

#[cfg(unix)]
mod raw {
    use std::fs::File;
    use std::io::{self, Seek, SeekFrom};
    use std::os::unix::fs::FileExt;
    use std::path::Path;

    #[derive(Debug)]
    pub struct RawInner {
        file: File,
        sector_size: u32,
    }

    pub fn open(path: &Path) -> crate::error::Result<(RawInner, u64, Option<String>)> {
        let file = File::open(path).map_err(|e| crate::error::Error::Io {
            offset: 0,
            source: e,
        })?;
        let mut f = file.try_clone().map_err(|e| crate::error::Error::Io {
            offset: 0,
            source: e,
        })?;
        let len = f
            .seek(SeekFrom::End(0))
            .map_err(|e| crate::error::Error::Io {
                offset: 0,
                source: e,
            })?;
        if len == 0 {
            return Err(crate::error::Error::invalid(
                "device reports zero length; pass the disk (e.g. /dev/sdb) rather than a partition \
                 if your platform needs that",
            ));
        }
        let sector_size = 512;
        Ok((
            RawInner { file, sector_size },
            len,
            Some(path.to_string_lossy().to_string()),
        ))
    }

    impl RawInner {
        pub fn sector_size(&self) -> u32 {
            self.sector_size
        }

        pub fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
            self.file.read_at(buf, offset)
        }
    }
}

#[cfg(not(any(windows, unix)))]
mod raw {
    use std::io;
    use std::path::Path;

    #[derive(Debug)]
    pub struct RawInner;

    pub fn open(_path: &Path) -> crate::error::Result<(RawInner, u64, Option<String>)> {
        Err(crate::error::Error::unsupported(
            "raw device access is not implemented on this platform",
        ))
    }

    impl RawInner {
        pub fn sector_size(&self) -> u32 {
            512
        }
        pub fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::Unsupported, "unsupported"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn file_source_is_bounded_and_read_only() {
        let dir = std::env::temp_dir().join(format!("relift-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.img");
        let mut f = File::create(&path).unwrap();
        f.write_all(&[0xAA; 4096]).unwrap();
        drop(f);

        let src = FileSource::open(&path).unwrap();
        assert_eq!(src.len(), 4096);
        let mut buf = [0u8; 16];
        assert_eq!(src.read_at(4090, &mut buf).unwrap(), 6);
        assert_eq!(src.read_at(4096, &mut buf).unwrap(), 0);
        assert_eq!(src.read_at(0, &mut []).unwrap(), 0);
        assert_eq!(src.identity().kind, SourceKind::Image);
        assert_eq!(src.identity().fingerprint.len(), 64);
    }

    #[test]
    fn truncated_reads_error() {
        let dir = std::env::temp_dir().join(format!("relift-src2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample2.img");
        std::fs::write(&path, [0u8; 8]).unwrap();
        let src = FileSource::open(&path).unwrap();
        let mut buf = [0u8; 16];
        assert!(matches!(
            src.read_exact_at(0, &mut buf),
            Err(Error::UnexpectedEof { .. })
        ));
    }

    #[test]
    fn device_detection_is_explicit() {
        #[cfg(windows)]
        {
            assert!(looks_like_device(Path::new(r"\\.\PhysicalDrive0")));
            assert!(!looks_like_device(Path::new(r"C:\image.dd")));
        }
        #[cfg(not(windows))]
        {
            assert!(looks_like_device(Path::new("/dev/sda")));
            assert!(!looks_like_device(Path::new("/tmp/image.dd")));
        }
    }
}
