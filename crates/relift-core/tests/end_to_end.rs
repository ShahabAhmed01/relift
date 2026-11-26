//! End-to-end recovery verification against every synthetic fixture.
//!
//! Each case scans a real image, recovers the resulting session, reads the
//! output bytes back, and compares both the bytes and the SHA-256 digest with
//! the fixture's independent expectation.

use std::fs;
use std::path::{Path, PathBuf};

use relift_core::candidate::EntryState;
use relift_core::recover::{run_recovery, RecoverOptions, RecoveryStatus};
use relift_core::scan::{run_scan, ScanProgress, ScanRequest};
use relift_core::source::{FileSource, ReadOnlySource};
use relift_testkit::{
    apfs_image, btrfs_image, exfat_image, ext4_image, fat32_image, hfsplus_image, ntfs_image,
    sha256_hex, xfs_image, FixtureFile,
};

fn case_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("relift-e2e-{}-{label}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn recover_fixture(label: &str, image: &[u8], files: &[FixtureFile]) {
    let base = case_dir(label);
    let image_path = base.join("source.img");
    fs::write(&image_path, image).unwrap();
    let src = FileSource::open(&image_path).unwrap();

    let session = base.join("session");
    let request = ScanRequest::new(&session);
    let mut scan_progress = |_p: ScanProgress| {};
    let header = run_scan(&src, &request, &mut scan_progress).unwrap();
    assert_eq!(header.status, relift_core::session::SessionStatus::Complete);

    let candidates = relift_core::session::load_candidates(&session).unwrap();
    let deleted: Vec<_> = candidates
        .iter()
        .filter(|c| c.state == EntryState::Deleted)
        .collect();
    assert!(
        !deleted.is_empty(),
        "{label}: scan produced no deleted candidates; candidates {candidates:?}"
    );

    let dest = base.join("out");
    fs::create_dir_all(&dest).unwrap();
    let options = RecoverOptions::new(&dest);
    let mut recovery_progress = |_p| {};
    let summary = run_recovery(&session, &src, &options, &mut recovery_progress).unwrap();
    assert_eq!(
        summary.failed, 0,
        "{label}: recovery failures: {:?}",
        summary.warnings
    );
    assert_eq!(
        summary.partial, 0,
        "{label}: fixture files should be fully recoverable: {:?}",
        summary.results
    );

    let expected: Vec<&FixtureFile> = files.iter().filter(|f| f.deleted).collect();
    for file in expected {
        let wanted = file.path.trim_start_matches('/');
        let candidate = candidates
            .iter()
            .find(|c| c.display_path() == wanted)
            .unwrap_or_else(|| {
                panic!(
                    "{label}: no candidate for {wanted}; candidates: {:?}",
                    candidates
                        .iter()
                        .map(|c| (c.display_path(), c.state))
                        .collect::<Vec<_>>()
                )
            });
        let record = summary
            .results
            .iter()
            .find(|r| r.id == candidate.id)
            .unwrap_or_else(|| {
                panic!(
                    "{label}: no recovery record for {wanted}; records: {:?}",
                    summary
                        .results
                        .iter()
                        .map(|r| (&r.original_path, r.status))
                        .collect::<Vec<_>>()
                )
            });
        assert_eq!(
            record.status,
            RecoveryStatus::Recovered,
            "{label}: {wanted} was not fully recovered: {record:?}"
        );
        assert!(
            record.verified,
            "{label}: {wanted} was not re-read verified"
        );
        let output = record
            .output_path
            .as_deref()
            .unwrap_or_else(|| panic!("{label}: {wanted} has no output path"));
        let bytes = fs::read(output).unwrap();
        assert_eq!(
            sha256_hex(&bytes),
            file.sha256,
            "{label}: recovered bytes for {wanted} do not match the fixture; expected {} got {} bytes at {output}",
            file.data.len(),
            bytes.len()
        );
        assert_eq!(bytes, file.data, "{label}: byte mismatch for {wanted}");
    }

    let _ = fs::remove_dir_all(&base);
}

fn path_exists(root: &Path, rel: &str) -> bool {
    root.join(rel).exists()
}

#[test]
fn recovers_fat32_fixture_bytes() {
    let fixture = fat32_image();
    recover_fixture("fat32", fixture.image(), &fixture.files);
}

#[test]
fn recovers_exfat_fixture_bytes() {
    let fixture = exfat_image();
    recover_fixture("exfat", fixture.image(), &fixture.files);
}

#[test]
fn recovers_ntfs_fixture_bytes() {
    let fixture = ntfs_image();
    recover_fixture("ntfs", fixture.image(), &fixture.files);
}

#[test]
fn recovers_ext4_fixture_bytes() {
    let fixture = ext4_image();
    recover_fixture("ext4", fixture.image(), &fixture.files);
}

#[test]
fn destination_separation_is_still_enforced_end_to_end() {
    let fixture = fat32_image();
    let base = case_dir("dest-safety");
    let image_path = base.join("source.img");
    fs::write(&image_path, fixture.image()).unwrap();
    let src = FileSource::open(&image_path).unwrap();
    let request = ScanRequest::new(base.join("session"));
    let mut progress = |_p: ScanProgress| {};
    run_scan(&src, &request, &mut progress).unwrap();

    // Recovery into the source image's own path must be refused.
    let mut options = RecoverOptions::new(&image_path);
    options.min_confidence = None;
    let mut recovery_progress = |_p| {};
    let err = run_recovery(
        &base.join("session"),
        &src,
        &options,
        &mut recovery_progress,
    )
    .unwrap_err();
    assert!(err.to_string().contains("not a directory"));
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn no_unexpected_files_are_written_outside_the_destination() {
    let fixture = ext4_image();
    let base = case_dir("confinement");
    let image_path = base.join("source.img");
    fs::write(&image_path, fixture.image()).unwrap();
    let src = FileSource::open(&image_path).unwrap();
    let session = base.join("session");
    let request = ScanRequest::new(&session);
    let mut progress = |_p: ScanProgress| {};
    run_scan(&src, &request, &mut progress).unwrap();
    let dest = base.join("out");
    fs::create_dir_all(&dest).unwrap();
    let options = RecoverOptions::new(&dest);
    let mut recovery_progress = |_p| {};
    run_recovery(&session, &src, &options, &mut recovery_progress).unwrap();
    assert!(path_exists(&dest, "deleted.txt"));
    // The original image must still be byte-identical after recovery.
    assert!(base.join("source.img").exists());
    let _ = fs::remove_dir_all(&base);
}

/// Probe identifies an XFS volume from its sector-0 magic.
#[test]
fn probe_identifies_xfs() {
    let fixture = xfs_image();
    let base = case_dir("probe-xfs");
    let image_path = base.join("source.img");
    fs::write(&image_path, fixture.image()).unwrap();
    let src = FileSource::open(&image_path).unwrap();
    let vol_ref = relift_core::fs::VolumeRef::new(0, src.len());
    let info = relift_core::fs::probe(&src, &vol_ref).unwrap();
    assert_eq!(info.kind, relift_core::fs::FsKind::Xfs);
    let _ = fs::remove_dir_all(&base);
}

/// Probe identifies a Btrfs volume from its superblock magic.
#[test]
fn probe_identifies_btrfs() {
    let fixture = btrfs_image();
    let base = case_dir("probe-btrfs");
    let image_path = base.join("source.img");
    fs::write(&image_path, fixture.image()).unwrap();
    let src = FileSource::open(&image_path).unwrap();
    let vol_ref = relift_core::fs::VolumeRef::new(0, src.len());
    let info = relift_core::fs::probe(&src, &vol_ref).unwrap();
    assert_eq!(info.kind, relift_core::fs::FsKind::Btrfs);
    let _ = fs::remove_dir_all(&base);
}

/// Probe identifies an HFS+ volume from its volume-header magic.
#[test]
fn probe_identifies_hfsplus() {
    let fixture = hfsplus_image();
    let base = case_dir("probe-hfsplus");
    let image_path = base.join("source.img");
    fs::write(&image_path, fixture.image()).unwrap();
    let src = FileSource::open(&image_path).unwrap();
    let vol_ref = relift_core::fs::VolumeRef::new(0, src.len());
    let info = relift_core::fs::probe(&src, &vol_ref).unwrap();
    assert_eq!(info.kind, relift_core::fs::FsKind::HfsPlus);
    let _ = fs::remove_dir_all(&base);
}

/// Probe identifies an APFS volume from its container magic.
#[test]
fn probe_identifies_apfs() {
    let fixture = apfs_image();
    let base = case_dir("probe-apfs");
    let image_path = base.join("source.img");
    fs::write(&image_path, fixture.image()).unwrap();
    let src = FileSource::open(&image_path).unwrap();
    let vol_ref = relift_core::fs::VolumeRef::new(0, src.len());
    let info = relift_core::fs::probe(&src, &vol_ref).unwrap();
    assert_eq!(info.kind, relift_core::fs::FsKind::Apfs);
    let _ = fs::remove_dir_all(&base);
}

/// scan_metadata reports experimental filesystems as identified-but-not-recovered.
#[test]
fn scan_metadata_reports_experimental_without_recovery() {
    let fixture = xfs_image();
    let base = case_dir("scan-xfs");
    let image_path = base.join("source.img");
    fs::write(&image_path, fixture.image()).unwrap();
    let src = FileSource::open(&image_path).unwrap();
    let vol_ref = relift_core::fs::VolumeRef::new(0, src.len());
    let mut sink = relift_core::fs::VecSink::default();
    let summary = relift_core::fs::scan_metadata(
        &src,
        &vol_ref,
        &relift_core::fs::FsScanOptions::default(),
        &mut sink,
    )
    .unwrap();
    assert_eq!(summary.kind, Some(relift_core::fs::FsKind::Xfs));
    // Experimental parsers should not emit candidates from metadata alone.
    assert_eq!(sink.items.len(), 0);
    let _ = fs::remove_dir_all(&base);
}
