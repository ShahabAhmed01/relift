use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use sha2::{Digest, Sha256};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "relift-cli-contract-{}-{serial}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create unique CLI contract test directory");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_relift"))
        .args(args)
        .output()
        .expect("launch relift CLI")
}

#[test]
fn version_json_has_stable_contract_fields() {
    let output = run_cli(&["--json", "version"]);

    assert_eq!(output.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&output.stdout).expect("version JSON");
    assert_eq!(value["tool"], "relift");
    assert!(value["version"].as_str().is_some());
    assert_eq!(value["schema"], 1);
    assert!(value["platform"].as_str().is_some());
    assert!(value["profile"].as_str().is_some());
}

#[test]
fn usage_and_runtime_failures_have_documented_exit_codes() {
    let usage = run_cli(&["not-a-command"]);
    assert_eq!(usage.status.code(), Some(2));

    let temp = TestDirectory::new();
    let missing_source = temp.path().join("missing.img");
    let session = temp.path().join("session");
    let runtime = Command::new(env!("CARGO_BIN_EXE_relift"))
        .arg("scan")
        .arg(missing_source)
        .arg("--session")
        .arg(session)
        .output()
        .expect("launch relift CLI");
    assert_eq!(runtime.status.code(), Some(1));
}

#[test]
fn scan_json_is_valid_and_recovery_rejects_a_changed_source() {
    let temp = TestDirectory::new();
    let image = temp.path().join("source.img");
    let session = temp.path().join("session");
    let destination = temp.path().join("destination");
    fs::create_dir(&destination).unwrap();
    let source_bytes = vec![0u8; 1024 * 1024];
    fs::write(&image, &source_bytes).unwrap();
    let source_hash = Sha256::digest(&source_bytes);

    let scan = Command::new(env!("CARGO_BIN_EXE_relift"))
        .arg("--json")
        .arg("scan")
        .arg(&image)
        .arg("--session")
        .arg(&session)
        .output()
        .expect("launch relift CLI");

    assert_eq!(scan.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&scan.stdout).expect("scan JSON");
    assert_eq!(value["tool"], "relift");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["status"], "complete");
    assert_eq!(value["source"]["kind"], "image");
    assert_eq!(value["source"]["size"], source_bytes.len());
    assert!(value["source"]["fingerprint"].as_str().is_some());
    assert!(value["summary"]["candidates"].as_u64().is_some());
    assert!(value["filesystems"].as_array().is_some());
    assert!(value["warnings"].as_array().is_some());
    assert_eq!(Sha256::digest(fs::read(&image).unwrap()), source_hash);

    let list = Command::new(env!("CARGO_BIN_EXE_relift"))
        .arg("--json")
        .arg("list")
        .arg(&session)
        .output()
        .expect("launch relift CLI");
    assert_eq!(list.status.code(), Some(0));
    let list_json: Value = serde_json::from_slice(&list.stdout).expect("list JSON");
    assert!(list_json["session"].as_str().is_some());
    assert_eq!(list_json["total"], 0);
    assert_eq!(list_json["shown"], 0);
    assert!(list_json["candidates"].as_array().is_some());

    let plan = Command::new(env!("CARGO_BIN_EXE_relift"))
        .arg("--json")
        .arg("recover")
        .arg(&session)
        .arg("--dest")
        .arg(&destination)
        .arg("--dry-run")
        .output()
        .expect("launch relift CLI");
    assert_eq!(plan.status.code(), Some(0));
    let plan_json: Value = serde_json::from_slice(&plan.stdout).expect("recovery JSON");
    assert_eq!(plan_json["dry_run"], true);
    assert!(plan_json["destination"].as_str().is_some());
    assert!(plan_json["results"].as_array().is_some());
    assert_eq!(plan_json["results_truncated"], false);
    assert!(plan_json["warnings"].as_array().is_some());
    assert!(plan_json["started"].as_str().is_some());
    assert!(plan_json["finished"].as_str().is_some());
    for field in [
        "considered",
        "attempted",
        "succeeded",
        "partial",
        "failed",
        "skipped",
        "bytes_written",
        "bytes_planned",
    ] {
        assert!(plan_json[field].as_u64().is_some(), "missing {field} field");
    }

    let mut changed_source = source_bytes;
    changed_source[0] = 1;
    fs::write(&image, changed_source).unwrap();
    let recovery = Command::new(env!("CARGO_BIN_EXE_relift"))
        .arg("--json")
        .arg("recover")
        .arg(&session)
        .arg("--dest")
        .arg(&destination)
        .output()
        .expect("launch relift CLI");

    assert_eq!(recovery.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&recovery.stderr).contains("source does not match"));
}
