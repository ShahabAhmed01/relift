//! Benchmarks for Relift recovery engine.
//!
//! Run with: `cargo bench -p relift-bench`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use relift_core::{
    carve::{scan_volume, CarveOptions},
    fs::{probe, scan_metadata, FsScanOptions, VolumeRef},
    source::FileSource,
};
use relift_testkit::{exfat_image, ext4_image, fat32_image, ntfs_image};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempfile::tempdir;

fn bench_signature_sweep(c: &mut Criterion) {
    let mut group = c.benchmark_group("signature_sweep");

    // Create a test buffer with many false positives
    let mut buf = vec![0u8; 64 * 1024 * 1024]; // 64 MB
                                               // Add JPEG signatures every 1 MB
    for i in 0..64 {
        let offset = i * 1024 * 1024;
        buf[offset..offset + 3].copy_from_slice(b"\xFF\xD8\xFF");
    }

    group.throughput(Throughput::Bytes(buf.len() as u64));
    group.bench_function("memchr_jpeg_64mb", |b| {
        b.iter(|| {
            let _hits: Vec<_> =
                memchr::memmem::find_iter(black_box(&buf), b"\xFF\xD8\xFF").collect();
        });
    });

    group.finish();
}

fn bench_carvers(c: &mut Criterion) {
    let mut group = c.benchmark_group("carvers");

    // JPEG validation
    let jpeg_data = create_test_jpeg(1024 * 1024); // 1 MB JPEG
    let dir = tempdir().unwrap();
    let jpeg_path = dir.path().join("test.jpg");
    std::fs::write(&jpeg_path, &jpeg_data).unwrap();
    let src = FileSource::open(&jpeg_path).unwrap();
    let volume = relift_core::fs::VolumeRef::new(0, jpeg_data.len() as u64);
    let options = CarveOptions::default();

    group.bench_function("validate_jpeg_1mb", |b| {
        b.iter(|| {
            let mut sink = relift_core::fs::VecSink::default();
            let mut progress = |_: u64, _: u64| {};
            let _ = relift_core::carve::scan_volume(
                black_box(&src),
                black_box(&volume),
                black_box(&options),
                black_box(&mut sink),
                black_box(&mut progress),
            );
        });
    });

    // ZIP validation
    let zip_data = create_test_zip(1024 * 1024);
    let zip_path = dir.path().join("test.zip");
    std::fs::write(&zip_path, &zip_data).unwrap();
    let zip_src = FileSource::open(&zip_path).unwrap();
    let zip_volume = relift_core::fs::VolumeRef::new(0, zip_data.len() as u64);

    group.bench_function("validate_zip_1mb", |b| {
        b.iter(|| {
            let mut sink = relift_core::fs::VecSink::default();
            let mut progress = |_: u64, _: u64| {};
            let _ = relift_core::carve::scan_volume(
                black_box(&zip_src),
                black_box(&zip_volume),
                black_box(&options),
                black_box(&mut sink),
                black_box(&mut progress),
            );
        });
    });

    group.finish();
}

fn bench_metadata_scan(c: &mut Criterion) {
    let mut group = c.benchmark_group("metadata_scan");

    // FAT32
    let fat32_fixture = fat32_image();
    let dir = tempdir().unwrap();
    let fat32_path = dir.path().join("fat32.img");
    std::fs::write(&fat32_path, fat32_fixture.image()).unwrap();
    let fat32_src = FileSource::open(&fat32_path).unwrap();
    let fat32_volume = relift_core::fs::VolumeRef::new(0, fat32_fixture.image().len() as u64);
    let fs_options = FsScanOptions::default();

    group.bench_function("fat32_metadata", |b| {
        b.iter(|| {
            let mut sink = relift_core::fs::VecSink::default();
            let _ = relift_core::fs::scan_metadata(
                black_box(&fat32_src),
                black_box(&fat32_volume),
                black_box(&fs_options),
                black_box(&mut sink),
            );
        });
    });

    // NTFS
    let ntfs_fixture = ntfs_image();
    let ntfs_path = dir.path().join("ntfs.img");
    std::fs::write(&ntfs_path, ntfs_fixture.image()).unwrap();
    let ntfs_src = FileSource::open(&ntfs_path).unwrap();
    let ntfs_volume = relift_core::fs::VolumeRef::new(0, ntfs_fixture.image().len() as u64);

    group.bench_function("ntfs_metadata", |b| {
        b.iter(|| {
            let mut sink = relift_core::fs::VecSink::default();
            let _ = relift_core::fs::scan_metadata(
                black_box(&ntfs_src),
                black_box(&ntfs_volume),
                black_box(&fs_options),
                black_box(&mut sink),
            );
        });
    });

    // ext4
    let ext4_fixture = ext4_image();
    let ext4_path = dir.path().join("ext4.img");
    std::fs::write(&ext4_path, ext4_fixture.image()).unwrap();
    let ext4_src = FileSource::open(&ext4_path).unwrap();
    let ext4_volume = relift_core::fs::VolumeRef::new(0, ext4_fixture.image().len() as u64);

    group.bench_function("ext4_metadata", |b| {
        b.iter(|| {
            let mut sink = relift_core::fs::VecSink::default();
            let _ = relift_core::fs::scan_metadata(
                black_box(&ext4_src),
                black_box(&ext4_volume),
                black_box(&fs_options),
                black_box(&mut sink),
            );
        });
    });

    group.finish();
}

fn bench_probe(c: &mut Criterion) {
    let mut group = c.benchmark_group("probe");

    let fixtures = [
        ("fat32", fat32_image()),
        ("exfat", exfat_image()),
        ("ntfs", ntfs_image()),
        ("ext4", ext4_image()),
    ];

    for (name, fixture) in fixtures {
        let dir = tempdir().unwrap();
        let path = dir.path().join(format!("{}.img", name));
        std::fs::write(&path, fixture.image()).unwrap();
        let src = FileSource::open(&path).unwrap();
        let volume = relift_core::fs::VolumeRef::new(0, fixture.image().len() as u64);

        group.bench_function(name, |b| {
            b.iter(|| {
                let _ = relift_core::fs::probe(black_box(&src), black_box(&volume));
            });
        });
    }

    group.finish();
}

fn create_test_jpeg(size: usize) -> Vec<u8> {
    // Minimal valid JPEG structure
    let mut data = vec![0u8; size];
    // SOI
    data[0] = 0xFF;
    data[1] = 0xD8;
    // APP0 marker
    data[2] = 0xFF;
    data[3] = 0xE0;
    data[4] = 0x00;
    data[4] = 0x10;
    data[6..16].copy_from_slice(b"JFIF\x00\x01\x01\x01\x00\x48\x00\x48\x00\x00");
    // SOF0
    let sof_offset = 16;
    data[sof_offset] = 0xFF;
    data[sof_offset + 1] = 0xC0;
    data[sof_offset + 2] = 0x00;
    data[sof_offset + 3] = 0x11;
    data[sof_offset + 4] = 0x08;
    data[sof_offset + 5] = 0x02;
    data[sof_offset + 6] = 0x80;
    data[sof_offset + 7] = 0x02;
    data[sof_offset + 8] = 0x80;
    data[sof_offset + 9] = 0x03;
    data[sof_offset + 10] = 0x01;
    data[sof_offset + 11] = 0x22;
    data[sof_offset + 12] = 0x00;
    data[sof_offset + 13] = 0x02;
    data[sof_offset + 14] = 0x11;
    data[sof_offset + 15] = 0x01;
    data[sof_offset + 16] = 0x03;
    data[sof_offset + 17] = 0x11;
    data[sof_offset + 18] = 0x01;
    data[sof_offset + 19] = 0xFF;
    data[sof_offset + 20] = 0xC4;
    // DHT (simplified)
    // SOS
    let sos_offset = size - 100;
    data[sos_offset] = 0xFF;
    data[sos_offset + 1] = 0xDA;
    data[sos_offset + 2] = 0x00;
    data[sos_offset + 3] = 0x0C;
    data[sos_offset + 4] = 0x03;
    data[sos_offset + 5] = 0x01;
    data[sos_offset + 6] = 0x00;
    data[sos_offset + 7] = 0x02;
    data[sos_offset + 8] = 0x11;
    data[sos_offset + 9] = 0x03;
    data[sos_offset + 10] = 0x11;
    data[sos_offset + 11] = 0x00;
    data[sos_offset + 12] = 0x3F;
    data[sos_offset + 13] = 0x00;
    // EOI
    data[size - 2] = 0xFF;
    data[size - 1] = 0xD9;
    data
}

fn create_test_zip(size: usize) -> Vec<u8> {
    // Minimal valid ZIP with one file
    let mut data = vec![0u8; size];
    // Local file header
    data[0..4].copy_from_slice(b"PK\x03\x04");
    data[4] = 0x14; // version
    data[5] = 0x00;
    data[6] = 0x00; // flags
    data[7] = 0x00;
    data[8] = 0x00; // compression
    data[9] = 0x00;
    data[10] = 0x00; // mod time
    data[11] = 0x00;
    data[12] = 0x00; // mod date
    data[13] = 0x00;
    data[14..18].copy_from_slice(&0u32.to_le_bytes()); // crc32
    data[18..22].copy_from_slice(&10u32.to_le_bytes()); // compressed size
    data[22..26].copy_from_slice(&10u32.to_le_bytes()); // uncompressed size
    data[26] = 0x08; // filename length
    data[27] = 0x00;
    data[28] = 0x00; // extra field length
    data[29] = 0x00;
    data[30..38].copy_from_slice(b"test.txt");
    // File data (10 bytes)
    data[38..48].copy_from_slice(b"HelloWorld");
    // Central directory
    let cd_offset = size - 200;
    data[cd_offset..cd_offset + 4].copy_from_slice(b"PK\x01\x02");
    data[cd_offset + 4] = 0x14;
    data[cd_offset + 5] = 0x00;
    data[cd_offset + 6] = 0x14;
    data[cd_offset + 7] = 0x00;
    data[cd_offset + 8] = 0x00;
    data[cd_offset + 9] = 0x00;
    data[cd_offset + 10] = 0x00;
    data[cd_offset + 11] = 0x00;
    data[cd_offset + 12] = 0x00;
    data[cd_offset + 13] = 0x00;
    data[cd_offset + 14] = 0x00;
    data[cd_offset + 15] = 0x00;
    data[cd_offset + 16..cd_offset + 20].copy_from_slice(&0u32.to_le_bytes());
    data[cd_offset + 20..cd_offset + 24].copy_from_slice(&10u32.to_le_bytes());
    data[cd_offset + 24..cd_offset + 28].copy_from_slice(&10u32.to_le_bytes());
    data[cd_offset + 28] = 0x08;
    data[cd_offset + 29] = 0x00;
    data[cd_offset + 30] = 0x00;
    data[cd_offset + 31] = 0x00;
    data[cd_offset + 32] = 0x00;
    data[cd_offset + 33] = 0x00;
    data[cd_offset + 34] = 0x00;
    data[cd_offset + 35] = 0x00;
    data[cd_offset + 36..cd_offset + 40].copy_from_slice(&0u32.to_le_bytes());
    data[cd_offset + 40..cd_offset + 44].copy_from_slice(&0u32.to_le_bytes());
    data[cd_offset + 44..cd_offset + 52].copy_from_slice(b"test.txt");
    // EOCD
    let eocd_offset = size - 22;
    data[eocd_offset..eocd_offset + 4].copy_from_slice(b"PK\x05\x06");
    data[eocd_offset + 4] = 0x00;
    data[eocd_offset + 5] = 0x00;
    data[eocd_offset + 6] = 0x00;
    data[eocd_offset + 7] = 0x00;
    data[eocd_offset + 8] = 0x01;
    data[eocd_offset + 9] = 0x00;
    data[eocd_offset + 10] = 0x01;
    data[eocd_offset + 11] = 0x00;
    data[eocd_offset + 12..eocd_offset + 16].copy_from_slice(&(52u32).to_le_bytes());
    data[eocd_offset + 16..eocd_offset + 20].copy_from_slice(&(cd_offset as u32).to_le_bytes());
    data[eocd_offset + 20] = 0x00;
    data[eocd_offset + 21] = 0x00;
    data
}

criterion_group!(
    benches,
    bench_signature_sweep,
    bench_carvers,
    bench_metadata_scan,
    bench_probe
);
criterion_main!(benches);
