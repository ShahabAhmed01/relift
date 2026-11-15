//! Bounds-checked little/big-endian readers and timestamp formatting.
//!
//! Every parser in Relift reads attacker-controlled bytes. These helpers return
//! `None` instead of panicking so a fuzzed or corrupted image can never bring
//! the process down through an out-of-range slice.

use crate::error::{Error, Result};

#[inline]
pub fn u16_le(buf: &[u8], off: usize) -> Option<u16> {
    let b = buf.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

#[inline]
pub fn u16_be(buf: &[u8], off: usize) -> Option<u16> {
    let b = buf.get(off..off.checked_add(2)?)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

#[inline]
pub fn u32_le(buf: &[u8], off: usize) -> Option<u32> {
    let b = buf.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[inline]
pub fn u32_be(buf: &[u8], off: usize) -> Option<u32> {
    let b = buf.get(off..off.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

#[inline]
pub fn u64_le(buf: &[u8], off: usize) -> Option<u64> {
    let b = buf.get(off..off.checked_add(8)?)?;
    Some(u64::from_le_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

#[inline]
pub fn u64_be(buf: &[u8], off: usize) -> Option<u64> {
    let b = buf.get(off..off.checked_add(8)?)?;
    Some(u64::from_be_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

#[inline]
pub fn i8_at(buf: &[u8], off: usize) -> Option<i8> {
    buf.get(off).copied().map(|v| v as i8)
}

#[inline]
pub fn u8_at(buf: &[u8], off: usize) -> Option<u8> {
    buf.get(off).copied()
}

#[inline]
pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    memchr::memmem::find(haystack, needle)
}

#[inline]
pub fn rfind_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    memchr::memmem::rfind(haystack, needle)
}

/// Round `value` up to a multiple of `align`. Returns `None` on overflow.
#[inline]
pub fn align_up(value: u64, align: u64) -> Option<u64> {
    if align == 0 {
        return None;
    }
    let rem = value % align;
    if rem == 0 {
        Some(value)
    } else {
        value.checked_add(align - rem)
    }
}

/// Checked range validation.
#[inline]
pub fn checked_range(offset: u64, len: u64, total: u64) -> Result<()> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| Error::Corrupt("offset arithmetic overflow".to_string()))?;
    if end > total {
        return Err(Error::Corrupt(format!(
            "structure claims bytes {offset:#x}..{end:#x} beyond source length {total:#x}"
        )));
    }
    Ok(())
}

/// Read a u16 from an owned buffer, or return an `UnexpectedEof` error.
pub fn require_u16_le(buf: &[u8], off: usize, what: &str) -> Result<u16> {
    u16_le(buf, off).ok_or_else(|| Error::Corrupt(format!("{what}: truncated u16 at {off}")))
}

pub fn require_u32_le(buf: &[u8], off: usize, what: &str) -> Result<u32> {
    u32_le(buf, off).ok_or_else(|| Error::Corrupt(format!("{what}: truncated u32 at {off}")))
}

pub fn require_u64_le(buf: &[u8], off: usize, what: &str) -> Result<u64> {
    u64_le(buf, off).ok_or_else(|| Error::Corrupt(format!("{what}: truncated u64 at {off}")))
}

/// Convert Unix seconds to a civil UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// Implemented locally to avoid a date/time dependency in the engine. Handles
/// the full range representable by `i64` seconds; returns `None` for years
/// outside 1..=9999.
pub fn unix_to_iso(secs: i64) -> Option<String> {
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    if !(1..=9999).contains(&y) {
        return None;
    }
    let hh = sod / 3600;
    let mm = (sod % 3600) / 60;
    let ss = sod % 60;
    Some(format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z"))
}

/// Howard Hinnant's `civil_from_days` algorithm.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Convert a Windows FILETIME (100 ns units since 1601-01-01) to ISO-8601 UTC.
pub fn filetime_to_iso(ft: u64) -> Option<String> {
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let secs = ft / 10_000_000;
    if secs < EPOCH_DIFF_SECS {
        return None;
    }
    unix_to_iso((secs - EPOCH_DIFF_SECS) as i64)
}

/// Convert a FAT date/time pair to ISO-8601-like local time without a zone.
///
/// FAT timestamps have no timezone. We deliberately do not pretend they do; the
/// returned string carries no `Z` suffix and callers label it as local time.
pub fn fat_datetime_to_iso(date: u16, time: u16) -> Option<String> {
    if date == 0 {
        return None;
    }
    let year = 1980 + ((date >> 9) & 0x7f) as i64;
    let month = ((date >> 5) & 0x0f) as u32;
    let day = (date & 0x1f) as u32;
    let hour = ((time >> 11) & 0x1f) as u32;
    let minute = ((time >> 5) & 0x3f) as u32;
    let second = ((time & 0x1f) * 2) as u32;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
    ))
}

/// Convert an exFAT timestamp (DOS date/time in a packed u32) to ISO-like local.
pub fn exfat_datetime_to_iso(raw: u32) -> Option<String> {
    if raw == 0 {
        return None;
    }
    let time = (raw & 0xffff) as u16;
    let date = (raw >> 16) as u16;
    fat_datetime_to_iso(date, time)
}

/// Human-readable byte count.
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Format seconds as `h:mm:ss` / `m:ss`.
pub fn human_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Decode UTF-16LE bytes into a `String`, replacing unpaired surrogates.
/// Decode a NUL-terminated UTF-16LE field.
///
/// Long-file-name entries pad the final slot with `0x0000` and then
/// `0xFFFF`; GPT label fields and exFAT names also use NUL termination.
/// Both padding forms are treated as end-of-string so recovered names never
/// carry replacement characters into the output path.
pub fn utf16le_to_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|u| *u != 0 && *u != 0xFFFF)
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_bounds_checked() {
        let buf = [1u8, 2, 3];
        assert_eq!(u16_le(&buf, 1), Some(0x0302));
        assert_eq!(u32_le(&buf, 1), None);
        assert_eq!(u64_le(&buf, 0), None);
        assert_eq!(i8_at(&buf, 9), None);
    }

    #[test]
    fn utf16_fields_stop_at_nul_or_ffff_padding() {
        assert_eq!(utf16le_to_string(b"n\0a\0m\0e\0\0\0"), "name");
        assert_eq!(utf16le_to_string(b"x\0y\0\xff\xff\xff\xff"), "xy");
    }

    #[test]
    fn epoch_formats_correctly() {
        assert_eq!(unix_to_iso(0).as_deref(), Some("1970-01-01T00:00:00Z"));
        assert_eq!(
            unix_to_iso(1_700_000_000).as_deref(),
            Some("2023-11-14T22:13:20Z")
        );
        assert_eq!(unix_to_iso(-1).as_deref(), Some("1969-12-31T23:59:59Z"));
    }

    #[test]
    fn filetime_epoch_converts() {
        // 2024-01-01T00:00:00Z
        assert_eq!(
            filetime_to_iso(133_485_408_000_000_000).as_deref(),
            Some("2024-01-01T00:00:00Z")
        );
    }

    #[test]
    fn align_and_ranges() {
        assert_eq!(align_up(5, 4), Some(8));
        assert_eq!(align_up(u64::MAX, 2), None);
        assert!(checked_range(10, 5, 16).is_ok());
        assert!(checked_range(10, 7, 16).is_err());
    }
}
