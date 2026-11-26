#![no_main]

use libfuzzer_sys::fuzz_target;
use relift_core::carve::{scan_volume, CarveOptions};
use relift_core::fs::{VolumeRef, VecSink};
use relift_core::source::{FileSource, ReadOnlySource};
use std::io::Write;

// Fuzz the validating signature carver with arbitrary bytes.
// The carver must never panic on malformed input and must only emit
// structurally validated candidates. Crashes here are release-blocking bugs.
fuzz_target!(|data: &[u8]| {
    let dir = std::env::temp_dir().join(format!(
        "relift-fuzz-carver-{}",
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
    let options = CarveOptions::default();
    let mut sink = VecSink::default();
    let mut progress = |_: u64, _: u64| {};
    let _ = scan_volume(&src, &volume, &options, &mut sink, &mut progress);
    let _ = std::fs::remove_file(&img);
});
