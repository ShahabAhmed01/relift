//! Destination safety and filename sanitization.
//!
//! Two independent rules are enforced here:
//!
//! 1. Recovery output must not be written back onto the evidence media. For a
//!    disk image this means "not the same file". For a raw device it means "not
//!    a volume on the same physical disk". The raw-device check fails closed:
//!    if the platform cannot prove the destination is on a different disk,
//!    recovery refuses and asks the user to image the device first.
//! 2. Names from damaged filesystems are untrusted. Every output component is
//!    sanitized, reserved device names are escaped, traversal components are
//!    removed, and the final path is confined to the destination root.

use std::fs;
use std::path::{Component, Path, PathBuf};

#[cfg(any(target_os = "linux", test))]
use std::collections::HashSet;

use crate::error::{Error, Result};
use crate::source::{SourceIdentity, SourceKind};

/// Maximum length (in bytes) of one sanitized path component.
pub const MAX_COMPONENT_BYTES: usize = 100;
/// Maximum number of directories preserved from a recovered path.
pub const MAX_PATH_DEPTH: usize = 12;

/// Validate a destination directory and prove it is separate from the source.
///
/// The directory must already exist. It is canonicalized before checks. For
/// raw devices the platform-specific disk mapping is verified; see the module
/// documentation for the fail-closed policy.
pub fn validate_destination(source: &SourceIdentity, dest: &Path) -> Result<PathBuf> {
    let meta = fs::metadata(dest).map_err(|e| {
        Error::UnsafeDestination(format!(
            "destination {} is not accessible: {e}",
            dest.display()
        ))
    })?;
    if !meta.is_dir() {
        return Err(Error::UnsafeDestination(format!(
            "destination {} is not a directory",
            dest.display()
        )));
    }
    let canonical = fs::canonicalize(dest).map_err(|e| {
        Error::UnsafeDestination(format!(
            "could not canonicalize destination {}: {e}",
            dest.display()
        ))
    })?;

    match source.kind {
        SourceKind::Image => {
            let source_path = fs::canonicalize(&source.path).map_err(|e| {
                Error::UnsafeDestination(format!(
                    "could not canonicalize source {}: {e}",
                    source.path
                ))
            })?;
            if canonical == source_path {
                return Err(Error::UnsafeDestination(
                    "destination is the source image itself".into(),
                ));
            }
            // Writing next to the image is allowed; every output file is opened
            // with create_new and a unique name, so the image itself can never
            // be overwritten by a colliding recovered filename.
        }
        SourceKind::RawDevice => {
            raw_destination_is_separate(&source.path, &canonical)?;
        }
    }
    Ok(canonical)
}

#[cfg(windows)]
fn raw_destination_is_separate(source_path: &str, dest: &Path) -> Result<()> {
    let source_disks = windows_disks::source_disk_numbers(source_path).map_err(|e| {
        Error::UnsafeDestination(format!(
            "could not determine which physical disk {} belongs to: {e}. Recovery from a raw \
             device requires proving the destination is on a different disk. Create an image \
             first, then scan the image.",
            source_path
        ))
    })?;
    let dest_disks = windows_disks::path_disk_numbers(dest).map_err(|e| {
        Error::UnsafeDestination(format!(
            "could not determine which physical disk the destination belongs to: {e}. Create an \
             image on a separately verified disk, then scan the image."
        ))
    })?;
    if source_disks.iter().any(|d| dest_disks.contains(d)) {
        return Err(Error::UnsafeDestination(format!(
            "destination {} is on physical disk {:?}, which also contains the evidence device {}. \
             Writing recovered files there could overwrite evidence. Use a different disk.",
            dest.display(),
            source_disks,
            source_path
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn raw_destination_is_separate(source_path: &str, dest: &Path) -> Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    let source_canonical =
        fs::canonicalize(source_path).unwrap_or_else(|_| PathBuf::from(source_path));
    let source_meta = fs::metadata(&source_canonical).map_err(|e| {
        Error::UnsafeDestination(format!(
            "could not stat evidence device {}: {e}",
            source_canonical.display()
        ))
    })?;
    if !source_meta.file_type().is_block_device() {
        return Err(Error::UnsafeDestination(format!(
            "{} is not a block device; refusing to treat it as a raw disk",
            source_canonical.display()
        )));
    }
    let info = linux_mount_for(dest).ok_or_else(|| {
        Error::UnsafeDestination(format!(
            "could not find the mount containing destination {} in /proc/self/mountinfo",
            dest.display()
        ))
    })?;
    if info.filesystem == "btrfs" {
        return Err(Error::UnsafeDestination(format!(
            "cannot prove that every Btrfs device backing destination {} is separate from the \
             evidence disk. Choose a destination on a filesystem with a directly resolvable \
             physical device, or scan an image instead.",
            dest.display()
        )));
    }
    let sysfs = Path::new("/sys");
    let mut destination_disks = HashSet::new();
    let mount_device = info.device.ok_or_else(|| {
        Error::UnsafeDestination(format!(
            "could not identify the backing device of destination {}; refusing to write to a \
             possibly-identical disk",
            dest.display()
        ))
    })?;
    destination_disks.extend(
        resolve_physical_disks(sysfs, mount_device).map_err(|error| {
            Error::UnsafeDestination(format!(
                "could not resolve the physical disk backing destination {}: {error}. Create an \
             image on a separately verified disk, then scan the image.",
                dest.display()
            ))
        })?,
    );
    if let Some(source) = &info.source {
        if let Ok(meta) = fs::metadata(source) {
            if meta.file_type().is_block_device() {
                destination_disks.extend(
                    resolve_physical_disks(sysfs, dev_t_pair(meta.rdev())).map_err(|error| {
                        Error::UnsafeDestination(format!(
                            "could not resolve mount source {} to a physical disk: {error}",
                            source.display()
                        ))
                    })?,
                );
            }
        }
        if fs::canonicalize(source).ok().as_deref() == Some(source_canonical.as_path()) {
            return Err(same_disk_error(source_path, dest));
        }
    }
    if destination_disks.is_empty() {
        return Err(Error::UnsafeDestination(format!(
            "could not identify a physical disk backing destination {}; refusing to write to a \
             possibly-identical disk",
            dest.display()
        )));
    }
    let source_disks =
        resolve_physical_disks(sysfs, dev_t_pair(source_meta.rdev())).map_err(|error| {
            Error::UnsafeDestination(format!(
                "could not resolve evidence device {} to its physical disk: {error}. Create a \
                 disk image first, then scan the image.",
                source_canonical.display()
            ))
        })?;
    if source_disks
        .iter()
        .any(|source_disk| destination_disks.contains(source_disk))
    {
        return Err(same_disk_error(source_path, dest));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn resolve_physical_disks(
    sysfs_root: &Path,
    device: (u64, u64),
) -> std::io::Result<HashSet<PathBuf>> {
    let device_path = sysfs_root
        .join("dev/block")
        .join(format!("{}:{}", device.0, device.1));
    let mut visited = HashSet::new();
    let mut physical_disks = HashSet::new();
    collect_physical_disks(sysfs_root, &device_path, &mut visited, &mut physical_disks)?;
    if physical_disks.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!(
                "device {}:{} has no identifiable physical backing disk",
                device.0, device.1
            ),
        ));
    }
    Ok(physical_disks)
}

#[cfg(any(target_os = "linux", test))]
fn collect_physical_disks(
    sysfs_root: &Path,
    device_path: &Path,
    visited: &mut HashSet<PathBuf>,
    physical_disks: &mut HashSet<PathBuf>,
) -> std::io::Result<()> {
    let device_path = fs::canonicalize(device_path)?;
    let devices_root = fs::canonicalize(sysfs_root.join("devices"))?;
    if !device_path.starts_with(&devices_root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "sysfs device path {} is outside /sys/devices",
                device_path.display()
            ),
        ));
    }
    if !visited.insert(device_path.clone()) {
        return Ok(());
    }

    let slaves_path = device_path.join("slaves");
    if slaves_path.is_dir() {
        let slaves = fs::read_dir(&slaves_path)?.collect::<std::io::Result<Vec<_>>>()?;
        if !slaves.is_empty() {
            for slave in slaves {
                collect_physical_disks(sysfs_root, &slave.path(), visited, physical_disks)?;
            }
            return Ok(());
        }
    }

    if device_path.join("partition").exists() {
        let parent = device_path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("partition {} has no parent disk", device_path.display()),
            )
        })?;
        return collect_physical_disks(sysfs_root, parent, visited, physical_disks);
    }

    if let Some(controller) = nvme_controller_for_namespace(&device_path) {
        physical_disks.insert(controller);
        return Ok(());
    }

    let virtual_root = fs::canonicalize(sysfs_root.join("devices/virtual"))?;
    if device_path.starts_with(&virtual_root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!(
                "virtual block device {} has no resolvable physical dependencies",
                device_path.display()
            ),
        ));
    }

    physical_disks.insert(device_path);
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn nvme_controller_for_namespace(device_path: &Path) -> Option<PathBuf> {
    let namespace = device_path.file_name()?.to_str()?;
    let controller_path = device_path.parent()?;
    let controller = controller_path.file_name()?.to_str()?;
    let controller_number = controller.strip_prefix("nvme")?;
    if controller_number.is_empty() || !controller_number.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let namespace_number = namespace.strip_prefix(controller)?.strip_prefix('n')?;
    if namespace_number.is_empty() || !namespace_number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(controller_path.to_path_buf())
}

#[cfg(target_os = "linux")]
fn dev_t_pair(dev: u64) -> (u64, u64) {
    // Linux's glibc dev_t encoding.
    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
    let minor = (dev & 0xff) | ((dev >> 12) & !0xff);
    (major, minor)
}

#[cfg(target_os = "linux")]
struct MountInfo {
    device: Option<(u64, u64)>,
    source: Option<PathBuf>,
    filesystem: String,
}

#[cfg(target_os = "linux")]
fn linux_mount_for(dest: &Path) -> Option<MountInfo> {
    let text = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let mut best: Option<(usize, MountInfo)> = None;
    for line in text.lines() {
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        let lf: Vec<&str> = left.split_whitespace().collect();
        let rf: Vec<&str> = right.split_whitespace().collect();
        if lf.len() < 5 || rf.len() < 2 {
            continue;
        }
        let mount_point = unescape_mount(lf[4]);
        if !dest.starts_with(&mount_point) {
            continue;
        }
        let device = lf[2]
            .split_once(':')
            .and_then(|(a, b)| Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?)));
        let source = rf[1];
        let source = if source.starts_with('/') {
            Some(unescape_mount(source))
        } else {
            None
        };
        let score = mount_point.components().count();
        let info = MountInfo {
            device,
            source,
            filesystem: rf[0].to_string(),
        };
        if best.as_ref().map(|(s, _)| score >= *s).unwrap_or(true) {
            best = Some((score, info));
        }
    }
    best.map(|(_, i)| i)
}

#[cfg(target_os = "linux")]
fn unescape_mount(s: &str) -> PathBuf {
    // mountinfo octal-escapes spaces and a few other characters as \040 etc.
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let octal = &s[i + 1..i + 4];
            if let Ok(v) = u8::from_str_radix(octal, 8) {
                out.push(v);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(all(unix, not(target_os = "linux")))]
fn raw_destination_is_separate(source_path: &str, dest: &Path) -> Result<()> {
    Err(Error::UnsafeDestination(format!(
        "Relift cannot prove that destination {} is on a different disk than {}, because this \
         platform's raw-device to mount mapping is not implemented. Create a disk image on a \
         separate drive, then scan the image.",
        dest.display(),
        source_path
    )))
}

#[cfg(not(any(windows, unix)))]
fn raw_destination_is_separate(source_path: &str, dest: &Path) -> Result<()> {
    Err(Error::UnsafeDestination(format!(
        "raw-device destination safety is not implemented on this platform ({} -> {})",
        source_path,
        dest.display()
    )))
}

#[cfg(unix)]
fn same_disk_error(source_path: &str, dest: &Path) -> Error {
    Error::UnsafeDestination(format!(
        "destination {} is on the same physical disk as evidence device {source_path}",
        dest.display()
    ))
}

#[cfg(windows)]
mod windows_disks {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    type Handle = *mut c_void;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct DiskExtent {
        disk_number: u32,
        starting_offset: i64,
        extent_length: i64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct VolumeDiskExtents {
        number_of_disk_extents: u32,
        extents: [DiskExtent; 8],
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
        fn CloseHandle(object: Handle) -> i32;
        fn GetVolumePathNameW(
            file_name: *const u16,
            volume_path_name: *mut u16,
            buffer_length: u32,
        ) -> i32;
    }

    fn wide(path: &str) -> Vec<u16> {
        std::ffi::OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn open_query_handle(path: &str) -> io::Result<Handle> {
        let w = wide(path);
        // SAFETY: w is NUL-terminated and valid for this call. Desired access is
        // zero: this handle can only be used for information queries, never for
        // writing or reading data.
        let h = unsafe {
            CreateFileW(
                w.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(h)
    }

    fn disk_numbers_for_handle(h: Handle) -> io::Result<Vec<u32>> {
        let mut out = VolumeDiskExtents::default();
        let mut returned = 0u32;
        // SAFETY: the handle is a valid query handle; out points to a correctly
        // sized, aligned buffer for this IOCTL.
        let ok = unsafe {
            DeviceIoControl(
                h,
                IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
                std::ptr::null_mut(),
                0,
                &mut out as *mut VolumeDiskExtents as *mut c_void,
                std::mem::size_of::<VolumeDiskExtents>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let n = (out.number_of_disk_extents as usize).min(out.extents.len());
        Ok(out.extents[..n].iter().map(|e| e.disk_number).collect())
    }

    fn volume_path_for(path: &Path) -> io::Result<String> {
        let mut display = path.to_string_lossy().to_string();
        for prefix in [r"\\?\UNC\", r"\\?\", r"\??\"] {
            if let Some(rest) = display.strip_prefix(prefix) {
                display = rest.to_string();
            }
        }
        let w = wide(&display);
        let mut buf = vec![0u16; 512];
        // SAFETY: w is NUL-terminated; buf is writable for its full length.
        let ok = unsafe { GetVolumePathNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..len]);
        Ok(s)
    }

    pub fn path_disk_numbers(path: &Path) -> io::Result<Vec<u32>> {
        let volume = volume_path_for(path)?;
        if volume.starts_with(r"\\") {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("volume path {volume} is a network share, not a local disk"),
            ));
        }
        let trimmed = volume.trim_end_matches('\\');
        let device = format!(r"\\.\{trimmed}");
        let h = open_query_handle(&device)?;
        let result = disk_numbers_for_handle(h);
        // SAFETY: h was returned by CreateFileW and is valid.
        unsafe {
            CloseHandle(h);
        }
        result
    }

    pub fn source_disk_numbers(source_path: &str) -> io::Result<Vec<u32>> {
        // A physical drive path identifies the disk directly.
        if let Some(num) = parse_physical_drive(source_path) {
            return Ok(vec![num]);
        }
        // A volume path: ask the volume which disk(s) back it.
        let h = open_query_handle(source_path)?;
        let result = disk_numbers_for_handle(h);
        // SAFETY: h is valid.
        unsafe {
            CloseHandle(h);
        }
        result
    }

    fn parse_physical_drive(s: &str) -> Option<u32> {
        let lower = s.to_ascii_lowercase();
        let idx = lower.find("physicaldrive")?;
        lower[idx + "physicaldrive".len()..]
            .trim_end_matches('\\')
            .parse()
            .ok()
    }
}

/// Remove traversal and dangerous characters from one filesystem path
/// component. Returns `None` for `.`/`..` and empty results.
pub fn sanitize_component(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        let bad = ch.is_control()
            || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            || ch == '\u{fffd}';
        out.push(if bad { '_' } else { ch });
    }
    // Windows strips trailing dots and spaces; make that explicit rather than
    // letting the OS silently alias two different recovered names.
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        return None;
    }
    if is_reserved_device_name(&out) {
        out.insert(0, '_');
    }
    Some(truncate_preserving_extension(&out, MAX_COMPONENT_BYTES))
}

fn is_reserved_device_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn truncate_preserving_extension(name: &str, max_bytes: usize) -> String {
    if name.len() <= max_bytes {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && e.len() <= 16 => (s, Some(e)),
        _ => (name, None),
    };
    let ext_len = ext.map(|e| e.len() + 1).unwrap_or(0);
    let budget = max_bytes.saturating_sub(ext_len).max(1);
    let mut end = budget.min(stem.len());
    while end > 0 && !stem.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = stem[..end].to_string();
    if let Some(e) = ext {
        out.push('.');
        out.push_str(e);
    }
    out
}

/// Sanitize a recovered relative path, preserving benign directory structure
/// while dropping traversal and limiting depth. Returns `None` when the path
/// contains no usable components.
pub fn sanitize_relative(raw: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for part in raw.split(['/', '\\']) {
        if depth >= MAX_PATH_DEPTH {
            break;
        }
        let Some(clean) = sanitize_component(part) else {
            continue;
        };
        // `sanitize_component` already rejects `.`/`..`; keep the explicit
        // Component check as defence in depth against platform path quirks.
        if matches!(
            Path::new(&clean).components().next(),
            Some(
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        ) {
            continue;
        }
        out.push(clean);
        depth += 1;
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Build a safe candidate output name from an untrusted recovered name.
pub fn sanitize_filename(name: &str, fallback: &str) -> String {
    sanitize_component(name)
        .or_else(|| sanitize_component(fallback))
        .unwrap_or_else(|| "recovered.bin".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_and_specials_are_removed() {
        assert_eq!(
            sanitize_relative("../../etc/passwd").unwrap(),
            PathBuf::from("etc/passwd")
        );
        assert_eq!(
            sanitize_relative(r"..\..\Windows\System32").unwrap(),
            PathBuf::from("Windows/System32")
        );
        assert!(sanitize_relative(".").is_none());
        assert!(sanitize_relative("..").is_none());
    }

    #[test]
    fn reserved_windows_names_are_escaped() {
        assert_eq!(sanitize_filename("CON.txt", "x").as_str(), "_CON.txt");
        assert_eq!(sanitize_filename("con", "x").as_str(), "_con");
        assert_eq!(sanitize_filename("COM1", "x").as_str(), "_COM1");
        assert_eq!(sanitize_filename("normal.txt", "x").as_str(), "normal.txt");
    }

    #[test]
    fn illegal_characters_and_trailing_dots_are_replaced() {
        let got = sanitize_filename("a:b*c?\"d<e>f|g\\h/i.", "fallback");
        assert!(!got.contains(':'));
        assert!(!got.contains('/'));
        assert!(!got.contains('\\'));
        assert!(!got.ends_with('.'));
        assert!(got.len() <= MAX_COMPONENT_BYTES);
    }

    #[test]
    fn long_names_keep_extension() {
        let name = format!("{}.jpg", "x".repeat(400));
        let got = sanitize_filename(&name, "fallback");
        assert!(got.ends_with(".jpg"));
        assert!(got.len() <= MAX_COMPONENT_BYTES);
    }

    #[test]
    fn linux_partition_mapping_groups_partitions_by_physical_disk() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "relift-sysfs-partitions-{}-{serial}",
            std::process::id()
        ));
        let sysfs = base.join("sys");
        let sda = sysfs.join("devices/pci0000/block/sda");
        let sda1 = sda.join("sda1");
        let sda2 = sda.join("sda2");
        let sdb = sysfs.join("devices/pci0000/block/sdb");
        let sdb1 = sdb.join("sdb1");
        fs::create_dir_all(&sda1).unwrap();
        fs::create_dir_all(&sda2).unwrap();
        fs::create_dir_all(&sdb1).unwrap();
        fs::create_dir_all(sysfs.join("devices/virtual/block")).unwrap();
        fs::write(sda1.join("partition"), "1").unwrap();
        fs::write(sda2.join("partition"), "2").unwrap();
        fs::write(sdb1.join("partition"), "1").unwrap();

        let mut source_seen = HashSet::new();
        let mut source_disks = HashSet::new();
        collect_physical_disks(&sysfs, &sda1, &mut source_seen, &mut source_disks).unwrap();
        let mut destination_seen = HashSet::new();
        let mut destination_disks = HashSet::new();
        collect_physical_disks(&sysfs, &sda2, &mut destination_seen, &mut destination_disks)
            .unwrap();
        let mut other_disk_seen = HashSet::new();
        let mut other_disk = HashSet::new();
        collect_physical_disks(&sysfs, &sdb1, &mut other_disk_seen, &mut other_disk).unwrap();

        assert_eq!(source_disks, destination_disks);
        assert!(source_disks.is_disjoint(&other_disk));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn linux_nvme_namespaces_map_to_the_same_controller() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("relift-sysfs-nvme-{}-{serial}", std::process::id()));
        let sysfs = base.join("sys");
        let controller = sysfs.join("devices/pci0000/nvme/nvme0");
        let namespace_one = controller.join("nvme0n1");
        let namespace_two = controller.join("nvme0n2");
        fs::create_dir_all(&namespace_one).unwrap();
        fs::create_dir_all(&namespace_two).unwrap();
        fs::create_dir_all(sysfs.join("devices/virtual")).unwrap();

        let mut first_seen = HashSet::new();
        let mut first_disks = HashSet::new();
        collect_physical_disks(&sysfs, &namespace_one, &mut first_seen, &mut first_disks).unwrap();
        let mut second_seen = HashSet::new();
        let mut second_disks = HashSet::new();
        collect_physical_disks(&sysfs, &namespace_two, &mut second_seen, &mut second_disks)
            .unwrap();

        assert_eq!(first_disks, second_disks);
        assert_eq!(
            first_disks,
            HashSet::from([controller.canonicalize().unwrap()])
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn linux_virtual_devices_without_dependencies_are_not_treated_as_physical_disks() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "relift-sysfs-virtual-{}-{serial}",
            std::process::id()
        ));
        let sysfs = base.join("sys");
        let loop_device = sysfs.join("devices/virtual/block/loop0");
        fs::create_dir_all(&loop_device).unwrap();

        let mut visited = HashSet::new();
        let mut physical_disks = HashSet::new();
        let result =
            collect_physical_disks(&sysfs, &loop_device, &mut visited, &mut physical_disks);

        assert!(result.is_err());
        assert!(physical_disks.is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_device_numbers_resolve_partition_links_to_parent_disks() {
        use std::os::unix::fs::symlink;
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "relift-sysfs-device-links-{}-{serial}",
            std::process::id()
        ));
        let sysfs = base.join("sys");
        let dev_block = sysfs.join("dev/block");
        let sda = sysfs.join("devices/pci0000/block/sda");
        let sda1 = sda.join("sda1");
        let sda2 = sda.join("sda2");
        let sdb = sysfs.join("devices/pci0000/block/sdb");
        let sdb1 = sdb.join("sdb1");
        fs::create_dir_all(&sda1).unwrap();
        fs::create_dir_all(&sda2).unwrap();
        fs::create_dir_all(&sdb1).unwrap();
        fs::create_dir_all(&dev_block).unwrap();
        fs::create_dir_all(sysfs.join("devices/virtual")).unwrap();
        fs::write(sda1.join("partition"), "1").unwrap();
        fs::write(sda2.join("partition"), "2").unwrap();
        fs::write(sdb1.join("partition"), "1").unwrap();
        symlink(&sda1, dev_block.join("8:1")).unwrap();
        symlink(&sda2, dev_block.join("8:2")).unwrap();
        symlink(&sdb1, dev_block.join("8:17")).unwrap();

        let source_disks = resolve_physical_disks(&sysfs, (8, 1)).unwrap();
        let same_disk = resolve_physical_disks(&sysfs, (8, 2)).unwrap();
        let other_disk = resolve_physical_disks(&sysfs, (8, 17)).unwrap();

        assert_eq!(source_disks, same_disk);
        assert!(source_disks.is_disjoint(&other_disk));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn image_destination_inside_source_parent_is_allowed_but_same_file_is_not() {
        let dir = std::env::temp_dir().join(format!("relift-safety-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let image = dir.join("source.img");
        fs::write(&image, b"x").unwrap();
        let out = dir.join("out");
        fs::create_dir_all(&out).unwrap();
        let identity = SourceIdentity {
            kind: SourceKind::Image,
            path: image.to_string_lossy().to_string(),
            device_id: None,
            size: 1,
            fingerprint: "f".repeat(64),
        };
        assert!(validate_destination(&identity, &out).is_ok());
        assert!(validate_destination(&identity, &dir).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }
}
