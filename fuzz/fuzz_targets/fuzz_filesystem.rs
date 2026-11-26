#![no_main]

use libfuzzer_sys::fuzz_target;
use relift_core::fs::{scan_metadata, VolumeRef, VecSink, FsScanOptions};
use relift_core::source::{FileSource, ReadOnlySource};
use std::io::Write;

/// Fuzz filesystem probing and metadata recovery with arbitrary bytes.
///
/// Probe and metadata scanning must never panic on malformed images, even for
/// filesystems that are only "identified but not implemented."
fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "relift-fuzz-filesystem-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let img = dir.join("fuzz.img");
    let mut f = std::fs::File::create(&img).unwrap();
    f.write_all(data).unwrap();
    drop(f);

    let src = match FileSource::open(&img) {
        Ok(s) => s,
        Err(_) => return,
    };
    let volume = VolumeRef::new(0, src.len());
    let mut sink = VecSink::default();
    let options = FsScanOptions::default();
    let _ = scan_metadata(&src, &volume, &options, &mut sink);
    let _ = std::fs::remove_file(&img);
});
