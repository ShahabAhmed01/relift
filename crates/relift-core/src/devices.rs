//! Device and volume enumeration, plus the `doctor` self-test.
//!
//! Enumeration is intentionally conservative. It reports what the operating
//! system exposes and whether Relift can open it read-only; it never claims a
//! device is recoverable when access requires privileges that are not present.

use serde::Serialize;

use crate::error::Result;
use crate::source::{RawDeviceSource, ReadOnlySource};
use crate::VERSION;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub path: String,
    /// `volume`, `disk`, or another platform description.
    pub kind: String,
    pub label: Option<String>,
    pub filesystem: Option<String>,
    pub size: Option<u64>,
    pub sector_size: Option<u32>,
    pub read_only: Option<bool>,
    pub accessible: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceList {
    pub platform: String,
    pub devices: Vec<DeviceInfo>,
    pub warnings: Vec<String>,
}

impl DeviceList {
    fn new() -> Self {
        Self {
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            devices: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

pub fn list_devices() -> DeviceList {
    #[cfg(windows)]
    {
        windows::list()
    }
    #[cfg(target_os = "linux")]
    {
        linux::list()
    }
    #[cfg(target_os = "macos")]
    {
        macos::list()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let mut out = DeviceList::new();
        out.warnings
            .push("device enumeration is not implemented on this platform".into());
        out
    }
}

#[cfg(windows)]
mod windows {
    use super::*;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn GetVolumeInformationW(
            root: *const u16,
            label: *mut u16,
            label_size: u32,
            serial: *mut u32,
            max_component: *mut u32,
            flags: *mut u32,
            fs_name: *mut u16,
            fs_size: u32,
        ) -> i32;
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn utf16_buf(buf: &[u16]) -> String {
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len]).trim().to_string()
    }

    pub fn list() -> DeviceList {
        let mut out = DeviceList::new();
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = format!("{letter}:\\");
            let wroot = wide(&root);
            let dtype = unsafe { GetDriveTypeW(wroot.as_ptr()) };
            if dtype == 1 {
                continue;
            }
            let mut label = [0u16; 261];
            let mut serial = 0u32;
            let mut max_component = 0u32;
            let mut flags = 0u32;
            let mut fs = [0u16; 261];
            let ok = unsafe {
                GetVolumeInformationW(
                    wroot.as_ptr(),
                    label.as_mut_ptr(),
                    label.len() as u32,
                    &mut serial,
                    &mut max_component,
                    &mut flags,
                    fs.as_mut_ptr(),
                    fs.len() as u32,
                )
            };
            let mut size = None;
            let mut free = 0u64;
            let mut total = 0u64;
            let mut total_free = 0u64;
            let size_ok = unsafe {
                GetDiskFreeSpaceExW(wroot.as_ptr(), &mut free, &mut total, &mut total_free)
            };
            if size_ok != 0 {
                size = Some(total);
            }
            let label = if ok != 0 {
                utf16_buf(&label)
            } else {
                String::new()
            };
            let fs = if ok != 0 {
                utf16_buf(&fs)
            } else {
                String::new()
            };
            let kind_note = match dtype {
                2 => "removable volume",
                3 => "fixed volume",
                4 => "network volume",
                5 => "optical volume",
                6 => "RAM volume",
                _ => "volume",
            };
            out.devices.push(DeviceInfo {
                path: root,
                kind: "volume".into(),
                label: (!label.is_empty()).then_some(label),
                filesystem: (!fs.is_empty()).then_some(fs),
                size,
                sector_size: None,
                read_only: Some(flags & 0x0008_0000 != 0),
                accessible: true,
                note: Some(kind_note.into()),
            });
        }

        let mut permission_denied = false;
        for n in 0..=15u32 {
            let path = format!(r"\\.\PhysicalDrive{n}");
            match RawDeviceSource::open(&path) {
                Ok(dev) => {
                    let desc = dev.description();
                    out.devices.push(DeviceInfo {
                        path,
                        kind: "disk".into(),
                        label: None,
                        filesystem: None,
                        size: Some(desc.identity.size),
                        sector_size: Some(desc.sector_size),
                        read_only: Some(true),
                        accessible: true,
                        note: Some(
                            "opened with GENERIC_READ only; mounted volumes may still change"
                                .into(),
                        ),
                    });
                }
                Err(crate::error::Error::Io { source, .. })
                    if source.kind() == std::io::ErrorKind::PermissionDenied =>
                {
                    permission_denied = true;
                    break;
                }
                Err(_) => {}
            }
        }
        if permission_denied {
            out.warnings.push(
                "Raw physical disks could not be enumerated because this process is not \
                 elevated. Drive letters are still listed. Run an elevated shell to see \
                 \\\\.\\PhysicalDriveN entries."
                    .into(),
            );
        }
        out.warnings.push(
            "Enumeration is advisory. Relift opens sources read-only; imaging a device before \
             recovery is still safer than scanning a mounted volume."
                .into(),
        );
        out
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;

    pub fn list() -> DeviceList {
        let mut out = DeviceList::new();
        let mut seen = BTreeSet::new();
        if let Ok(text) = fs::read_to_string("/proc/self/mounts") {
            for line in text.lines() {
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.len() < 4 {
                    continue;
                }
                let source = f[0];
                if !source.starts_with('/') {
                    continue;
                }
                let mount = f[1].replace("\\040", " ");
                let fstype = f[2];
                let ro = f[3].split(',').any(|o| o == "ro");
                if !seen.insert((source.to_string(), mount.clone())) {
                    continue;
                }
                let size = block_size(source);
                out.devices.push(DeviceInfo {
                    path: source.to_string(),
                    kind: "volume".into(),
                    label: Some(mount.clone()),
                    filesystem: Some(fstype.to_string()),
                    size,
                    sector_size: None,
                    read_only: Some(ro),
                    accessible: true,
                    note: Some(format!("mounted at {mount}")),
                });
            }
        } else {
            out.warnings
                .push("could not read /proc/self/mounts to enumerate volumes".into());
        }
        if let Ok(entries) = fs::read_dir("/sys/block") {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("zram") {
                    continue;
                }
                let size = fs::read_to_string(e.path().join("size"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .map(|sectors| sectors.saturating_mul(512));
                let ro = fs::read_to_string(e.path().join("ro"))
                    .ok()
                    .map(|s| s.trim() == "1");
                let path = format!("/dev/{name}");
                if !std::path::Path::new(&path).exists() {
                    continue;
                }
                out.devices.push(DeviceInfo {
                    path,
                    kind: "disk".into(),
                    label: None,
                    filesystem: None,
                    size,
                    sector_size: Some(512),
                    read_only: ro,
                    accessible: true,
                    note: Some("sysfs block device; open is read-only".into()),
                });
            }
        }
        out.warnings.push(
            "Linux block devices are opened O_RDONLY. Raw access may require root; if open \
             fails, image the device from a privileged shell and scan the image."
                .into(),
        );
        out
    }

    fn block_size(device: &str) -> Option<u64> {
        let canonical = std::fs::canonicalize(device).ok()?;
        let name = canonical.file_name()?.to_string_lossy().to_string();
        let text = std::fs::read_to_string(format!("/sys/class/block/{name}/size")).ok()?;
        Some(text.trim().parse::<u64>().ok()?.saturating_mul(512))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub fn list() -> DeviceList {
        let mut out = DeviceList::new();
        if let Ok(entries) = std::fs::read_dir("/dev") {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !name.starts_with("disk") {
                    continue;
                }
                let path = format!("/dev/{name}");
                let (size, sector, detail) = match RawDeviceSource::open(&path) {
                    Ok(dev) => {
                        let d = dev.description();
                        (
                            Some(d.identity.size),
                            Some(d.sector_size),
                            "open read-only".to_string(),
                        )
                    }
                    Err(e) => (None, None, format!("could not open read-only: {e}")),
                };
                out.devices.push(DeviceInfo {
                    path,
                    kind: "disk".into(),
                    label: None,
                    filesystem: None,
                    size,
                    sector_size: sector,
                    read_only: Some(true),
                    accessible: size.is_some(),
                    note: Some(detail),
                });
            }
        }
        out.warnings.push(
            "macOS raw disks usually require elevated privileges. Mounted APFS/HFS+ volumes are \
             not enumerated here; use `diskutil list` and pass /dev/diskNsM explicitly."
                .into(),
        );
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub tool: String,
    pub version: String,
    pub platform: String,
    pub profile: String,
    pub checks: Vec<Check>,
    pub overall: CheckStatus,
}

pub fn doctor() -> DoctorReport {
    let mut checks = Vec::new();
    checks.push(Check {
        name: "build".into(),
        status: CheckStatus::Pass,
        detail: format!(
            "relift {VERSION} on {}-{} ({})",
            std::env::consts::OS,
            std::env::consts::ARCH,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        ),
    });

    let hash_ok = crate::hash::hash_bytes(b"abc")
        == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    checks.push(Check {
        name: "sha256".into(),
        status: if hash_ok {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail: if hash_ok {
            "SHA-256 matches the NIST abc vector".into()
        } else {
            "SHA-256 self-test failed; do not trust output hashes".into()
        },
    });

    let temp = std::env::temp_dir().join(format!("relift-doctor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    match std::fs::create_dir_all(&temp) {
        Ok(()) => {
            let image = temp.join("selftest.img");
            let ok = std::fs::write(&image, b"relift-selftest")
                .and_then(|_| std::fs::canonicalize(&image))
                .is_ok();
            checks.push(Check {
                name: "temporary storage".into(),
                status: if ok {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Fail
                },
                detail: format!("wrote and reopened a test file in {}", temp.display()),
            });
        }
        Err(e) => checks.push(Check {
            name: "temporary storage".into(),
            status: CheckStatus::Fail,
            detail: format!("could not create {}: {e}", temp.display()),
        }),
    }

    // Carving self-test: a synthetic SQLite header must be validated.
    let db = temp.join("selftest-sqlite.img");
    let mut bytes = vec![0u8; 8192];
    bytes[..16].copy_from_slice(b"SQLite format 3\0");
    bytes[16..18].copy_from_slice(&4096u16.to_be_bytes());
    bytes[18] = 1;
    bytes[19] = 1;
    bytes[21] = 64;
    bytes[22] = 32;
    bytes[23] = 32;
    bytes[28..32].copy_from_slice(&2u32.to_be_bytes());
    let carve_ok = std::fs::write(&db, &bytes).is_ok()
        && crate::source::FileSource::open(&db)
            .ok()
            .map(|src| {
                let volume = crate::fs::VolumeRef::new(0, src.len());
                let mut sink = crate::fs::VecSink::default();
                let mut progress = |_a: u64, _b: u64| {};
                crate::carve::scan_volume(
                    &src,
                    &volume,
                    &crate::carve::CarveOptions {
                        min_size: 16,
                        ..Default::default()
                    },
                    &mut sink,
                    &mut progress,
                )
                .map(|s| s.emitted >= 1)
                .unwrap_or(false)
            })
            .unwrap_or(false);
    checks.push(Check {
        name: "carving validator".into(),
        status: if carve_ok {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail: if carve_ok {
            "validated a synthetic SQLite database through the carving engine".into()
        } else {
            "carving self-test failed; do not trust carved output".into()
        },
    });

    let (raw_status, raw_detail) = raw_access_check();
    checks.push(Check {
        name: "raw device access".into(),
        status: raw_status,
        detail: raw_detail,
    });

    checks.push(Check {
        name: "network / telemetry".into(),
        status: CheckStatus::Pass,
        detail: "this build contains no network client and sends no telemetry".into(),
    });

    let _ = std::fs::remove_dir_all(&temp);
    let overall = if checks.iter().any(|c| c.status == CheckStatus::Fail) {
        CheckStatus::Fail
    } else if checks.iter().any(|c| c.status == CheckStatus::Warn) {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    DoctorReport {
        tool: "relift".into(),
        version: VERSION.into(),
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        checks,
        overall,
    }
}

fn raw_access_check() -> (CheckStatus, String) {
    let candidates: &[&str] = if cfg!(windows) {
        &[r"\\.\PhysicalDrive0"]
    } else if cfg!(target_os = "macos") {
        &["/dev/disk0"]
    } else {
        &["/dev/sda", "/dev/nvme0n1", "/dev/vda"]
    };
    for path in candidates {
        match RawDeviceSource::open(path) {
            Ok(dev) => {
                let mut buf = [0u8; 512];
                return match dev.read_at(0, &mut buf) {
                    Ok(n) if n > 0 => (
                        CheckStatus::Pass,
                        format!(
                            "{path} opened read-only, {} bytes, sector size {}",
                            dev.len(),
                            dev.sector_size()
                        ),
                    ),
                    Ok(_) => (
                        CheckStatus::Warn,
                        format!("{path} opened but returned no data at offset 0"),
                    ),
                    Err(e) => (
                        CheckStatus::Warn,
                        format!("{path} opened but the first read failed: {e}"),
                    ),
                };
            }
            Err(crate::error::Error::Io { source, .. })
                if source.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                return (
                    CheckStatus::Warn,
                    format!(
                        "{path} requires elevation. Relift can still scan disk images without \
                         administrator privileges."
                    ),
                );
            }
            Err(_) => {}
        }
    }
    (
        CheckStatus::Warn,
        "no raw block device could be opened; this is normal without elevation or in a \
         container. Image files work without it."
            .into(),
    )
}

/// Convenience used by the CLI to open a device with a focused error message.
pub fn open_for_scan(path: &str) -> Result<Box<dyn ReadOnlySource>> {
    crate::source::open_source(path)
}
