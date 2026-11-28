//! Relift command-line interface.
//!
//! Every command that reads evidence goes through the same read-only core used
//! by the tests. The CLI never writes to a source: `scan` streams candidates
//! into a session directory, `recover` writes only below an explicitly chosen
//! destination, and `image` writes a new file with `create_new`.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
use relift_core::candidate::ConfidenceLabel;
use relift_core::devices::{doctor, list_devices, CheckStatus};
use relift_core::error::Error;
use relift_core::filter::CandidateFilter;
use relift_core::imaging::{create_image, ImageOptions};
use relift_core::partitions::{read_partition_table, scan_lost_partitions};
use relift_core::recover::{run_recovery, RecoverOptions, RecoveryStatus, RecoverySummary};
use relift_core::report::{write_report, ReportFormat};
use relift_core::scan::{run_scan, ScanProgress, ScanRequest};
use relift_core::session::{load_header, ManualVolume, SessionHeader, SessionStatus};
use relift_core::source::open_source;
use relift_core::util::{human_bytes, human_duration};
use relift_core::virtual_disk::{open_virtual_disk, open_forensic_image};
use relift_core::{SCHEMA_VERSION, VERSION};
use serde::Serialize;

mod config;

/// Stable process exit codes. These are part of the CLI contract and are
/// documented in `docs/CLI.md`.
const EXIT_OK: u8 = 0;
const EXIT_RUNTIME: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_UNSAFE: u8 = 3;
const EXIT_MISMATCH: u8 = 4;
const EXIT_PARTIAL: u8 = 5;

#[derive(Debug, Parser)]
#[command(
    name = "relift",
    version = VERSION,
    about = "Read-only deleted-file recovery for disk images and drives",
    long_about = "Relift scans disk images and raw devices without writing to them, records what it \
                  finds in a session directory, and recovers validated byte ranges only to a \
                  separate destination. Unsupported filesystems and unvalidated carves are \
                  reported, never silently presented as recovered."
)]
struct Cli {
    /// Emit machine-readable JSON on stdout.
    #[arg(long, global = true)]
    json: bool,

    /// Path to a relift.toml configuration file. Defaults to ./relift.toml or
    /// $HOME/.config/relift/config.toml. CLI arguments override config values.
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Probe filesystems and write deleted-file candidates to a session.
    Scan(ScanArgs),
    /// List candidates saved in a session without writing anything.
    List(ListArgs),
    /// Recover candidates from a session into a destination directory.
    Recover(RecoverArgs),
    /// Copy a source to a new image file and write a SHA-256 manifest.
    Image(ImageArgs),
    /// List volumes and readable disks exposed by the operating system.
    Devices,
    /// Run self-tests for hashing, carving, storage and raw access.
    Doctor,
    /// Render a saved session as JSON, HTML or CSV.
    Report(ReportArgs),
    /// Generate shell completions.
    Completions(CompletionsArgs),
    /// Print version and schema information.
    Version,
    /// Show partition table (MBR/GPT/APM) and probe filesystems.
    Partitions(PartitionsArgs),
    /// Scan for lost/deleted partitions by filesystem signature.
    PartitionsScan(PartitionsScanArgs),
    /// Inspect virtual disk image (VHD/VHDX/VMDK/QCOW/VDI).
    VirtualDisk(VirtualDiskArgs),
    /// Inspect forensic image (E01/Ex01/AFF4).
    ForensicImage(ForensicImageArgs),
    /// Inspect a source or session: identity, fingerprint, partition layout.
    Info(InfoArgs),
    /// Verify a session's recovered files against stored SHA-256 hashes.
    Verify(VerifyArgs),
    /// Manage and inspect recovery sessions.
    Session(SessionArgs),
    /// Check for and apply updates from GitHub releases.
    SelfUpdate(SelfUpdateArgs),
    /// Run a read-only MCP server exposing recovery tools.
    Mcp(McpArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ScanMode {
    /// Metadata recovery only. Never reads the whole volume.
    Quick,
    /// Metadata recovery plus validated signature carving.
    Deep,
}

#[derive(Debug, Args)]
struct ScanArgs {
    /// Disk image or raw device path.
    source: PathBuf,

    /// New or empty directory for session output (existing directory with --resume).
    #[arg(long, value_name = "DIR")]
    session: PathBuf,

    /// Resume an interrupted scan in an existing session directory. The
    /// session's recorded scan options are used.
    #[arg(long)]
    resume: bool,

    /// Scan preset: quick (metadata only) or deep (metadata + carving).
    #[arg(long, value_enum, default_value_t = ScanMode::Quick, value_name = "MODE")]
    mode: ScanMode,

    /// Also list files that are still allocated.
    #[arg(long)]
    live: bool,

    /// Run signature carving in addition to metadata recovery.
    #[arg(long)]
    carve: bool,

    /// Scan only this volume index (repeatable).
    #[arg(long = "volume", value_name = "INDEX")]
    volume: Vec<u32>,

    /// Manual volume offset in bytes (requires --length).
    #[arg(long, value_name = "BYTES", requires = "length")]
    offset: Option<String>,

    /// Manual volume length in bytes (requires --offset).
    #[arg(long, value_name = "BYTES", requires = "offset")]
    length: Option<String>,

    /// Stop after this many candidates.
    #[arg(long, default_value_t = 1_000_000, value_name = "N")]
    max_candidates: usize,
}

#[derive(Debug, Args)]
struct ListArgs {
    /// Session directory produced by `relift scan`.
    session: PathBuf,

    /// Filter candidates (repeatable): ext:jpg,png, format:pdf, name:IMG_, path:DCIM.
    #[arg(long, value_name = "EXPR")]
    filter: Vec<String>,

    /// Hide candidates below this confidence.
    #[arg(long, value_name = "LABEL")]
    min_confidence: Option<String>,

    /// Maximum rows to print (0 means all).
    #[arg(long, default_value_t = 100, value_name = "N")]
    limit: usize,
}

#[derive(Debug, Args)]
struct RecoverArgs {
    /// Session directory produced by `relift scan`.
    session: PathBuf,

    /// Destination directory. Must be separate from the evidence.
    #[arg(long, value_name = "DIR")]
    dest: PathBuf,

    /// Plan only: validate source and destination and list what would be written.
    #[arg(long)]
    dry_run: bool,

    /// Filter candidates (repeatable): ext:jpg,png, format:pdf, name:IMG_, path:DCIM.
    #[arg(long, value_name = "EXPR")]
    filter: Vec<String>,

    /// Re-open this source instead of the path recorded in the session.
    #[arg(long, value_name = "PATH")]
    source: Option<PathBuf>,

    /// Do not re-read each output file after writing it.
    #[arg(long)]
    no_verify: bool,

    /// Skip candidates below this confidence.
    #[arg(long, value_name = "LABEL")]
    min_confidence: Option<String>,

    /// Recover only these candidate ids (repeatable).
    #[arg(long = "id", value_name = "N")]
    id: Vec<u64>,
}

#[derive(Debug, Args)]
struct CompletionsArgs {
    /// Shell to generate completions for.
    #[arg(value_enum)]
    shell: Shell,
}

#[derive(Debug, Args)]
struct ImageArgs {
    /// Disk image or raw device path.
    source: PathBuf,

    /// New image file to create. Existing files are never overwritten.
    #[arg(long, value_name = "FILE")]
    dest: PathBuf,

    /// Do not re-read the finished image to verify its hash.
    #[arg(long)]
    no_verify: bool,
}

#[derive(Debug, Args)]
struct ReportArgs {
    /// Session directory produced by `relift scan`.
    session: PathBuf,

    /// Report format: json, html or csv.
    #[arg(long, default_value = "json", value_name = "FORMAT")]
    format: String,

    /// Output file. Defaults to `<session>/report.<format>`.
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,

    /// Maximum candidates to list (0 means all).
    #[arg(long, default_value_t = 0, value_name = "N")]
    limit: usize,
}

#[derive(Debug, Args)]
struct PartitionsArgs {
    /// Disk image or raw device path.
    source: PathBuf,

    /// Also probe filesystem on each partition.
    #[arg(long)]
    probe: bool,
}

#[derive(Debug, Args)]
struct PartitionsScanArgs {
    /// Disk image or raw device path.
    source: PathBuf,

    /// Maximum bytes to scan (default: 1 TB).
    #[arg(long, value_name = "BYTES")]
    max_bytes: Option<String>,

    /// Output session directory for found partitions (optional).
    #[arg(long, value_name = "DIR")]
    session: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct VirtualDiskArgs {
    /// Virtual disk image path.
    source: PathBuf,

    /// Show detailed info.
    #[arg(long)]
    verbose: bool,
}

#[derive(Debug, Args)]
struct ForensicImageArgs {
    /// Forensic image path.
    source: PathBuf,

    /// Show detailed info.
    #[arg(long)]
    verbose: bool,
}

#[derive(Debug, Args)]
struct InfoArgs {
    /// Disk image, raw device, or session directory to inspect.
    source: PathBuf,

    /// Show detailed fingerprint and partition information.
    #[arg(long)]
    verbose: bool,
}

#[derive(Debug, Args)]
struct VerifyArgs {
    /// Session directory produced by `relift scan`/`relift recover`.
    session: PathBuf,

    /// Re-read recovered files and compare SHA-256 against stored hashes.
    #[arg(long)]
    verify_hashes: bool,
}

#[derive(Debug, Args)]
struct SessionArgs {
    /// Session directory to inspect.
    session: PathBuf,

    /// Show full candidate listing (can be large).
    #[arg(long)]
    full: bool,
}

#[derive(Debug, Args)]
struct SelfUpdateArgs {
    /// Check for updates without applying.
    #[arg(long)]
    check: bool,

    /// GitHub repository in `owner/repo` form.
    #[arg(long, default_value = "ShahabAhmed01/relift")]
    repo: String,
}

#[derive(Debug, Args)]
struct McpArgs {
    /// Transport: stdio (default) or tcp.
    #[arg(long, default_value = "stdio")]
    transport: String,

    /// TCP listen address (only used with --transport tcp).
    #[arg(long, default_value = "127.0.0.0:8765")]
    listen: String,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("relift: {error:#}");
            ExitCode::from(exit_code_for(&error))
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<u8> {
    let config = config::load_config(cli.config.as_deref())?;
    match cli.command {
        Command::Scan(args) => cmd_scan(args, cli.json, &config),
        Command::List(args) => cmd_list(args, cli.json),
        Command::Recover(args) => cmd_recover(args, cli.json, &config),
        Command::Image(args) => cmd_image(args, cli.json),
        Command::Devices => cmd_devices(cli.json),
        Command::Doctor => cmd_doctor(cli.json),
        Command::Report(args) => cmd_report(args, cli.json),
        Command::Completions(args) => cmd_completions(args),
        Command::Version => cmd_version(cli.json),
        Command::Partitions(args) => cmd_partitions(args, cli.json),
        Command::PartitionsScan(args) => cmd_partitions_scan(args, cli.json),
        Command::VirtualDisk(args) => cmd_virtual_disk(args, cli.json),
        Command::ForensicImage(args) => cmd_forensic_image(args, cli.json),
        Command::Info(args) => cmd_info(args, cli.json),
        Command::Verify(args) => cmd_verify(args, cli.json),
        Command::Session(args) => cmd_session(args, cli.json),
        Command::SelfUpdate(args) => cmd_self_update(args, cli.json),
        Command::Mcp(args) => cmd_mcp(args, cli.json),
    }
}

fn cmd_scan(args: ScanArgs, json: bool, config: &config::Config) -> anyhow::Result<u8> {
    let manual_volume = match (args.offset.as_deref(), args.length.as_deref()) {
        (Some(offset), Some(length)) => Some(ManualVolume {
            offset: parse_bytes(offset, "offset")?,
            length: parse_bytes(length, "length")?,
        }),
        (None, None) => None,
        _ => unreachable!("clap enforces --offset/--length together"),
    };
    let src = open_source(&args.source)
        .with_context(|| format!("could not open source {}", args.source.display()))?;
    let identity = src.identity();
    if identity.kind == relift_core::source::SourceKind::RawDevice {
        eprintln!(
            "warning: {} is a raw device. Relift opens it read-only, but a mounted filesystem can \
             still change underneath. Imaging first is safer.",
            args.source.display()
        );
    }
    let mut request = ScanRequest::new(&args.session);
    // Config-file defaults apply first; CLI arguments override them.
    request.include_live = config.scan.include_live.unwrap_or(args.live);
    if config.scan.carve.unwrap_or(false) {
        request.carve = true;
    }
    request.carve = args.carve || request.carve || args.mode == ScanMode::Deep;
    request.volume_indices = args.volume.clone();
    request.manual_volume = manual_volume;
    request.max_candidates = config
        .scan
        .max_candidates
        .unwrap_or(args.max_candidates)
        .max(1);
    request.resume = args.resume;

    // Config-file scan mode ("quick" or "deep") enables carving when deep.
    if let Some(mode) = &config.scan.mode {
        let mode_deep = mode.trim().to_ascii_lowercase() == "deep";
        if mode_deep {
            request.carve = true;
        }
    }

    let mut printer = Progress::new(!json);
    let header = {
        let mut progress = |p: ScanProgress| printer.update(p.phase, p.done, p.total);
        run_scan(src.as_ref(), &request, &mut progress)?
    };
    printer.finish();

    if json {
        print_json(&header)?;
    } else {
        print_scan_summary(&header, &args.session);
    }
    Ok(if header.status == SessionStatus::Failed {
        EXIT_PARTIAL
    } else {
        EXIT_OK
    })
}

fn cmd_recover(args: RecoverArgs, json: bool, config: &config::Config) -> anyhow::Result<u8> {
    let header = load_header(&args.session)
        .with_context(|| format!("could not read session {}", args.session.display()))?;
    let source_path = args
        .source
        .clone()
        .unwrap_or_else(|| PathBuf::from(&header.source.path));
    let src = open_source(&source_path)
        .with_context(|| format!("could not open source {}", source_path.display()))?;
    let min_confidence = match args.min_confidence.as_deref() {
        Some(value) => Some(parse_confidence(value)?),
        None => config
            .recover
            .min_confidence
            .as_deref()
            .map(parse_confidence)
            .transpose()?,
    };
    let ids = if args.id.is_empty() {
        None
    } else {
        Some(args.id.iter().copied().collect())
    };
    let filter = CandidateFilter::parse_all(&args.filter)?;
    let dest = prepare_destination(&args.dest)?;
    let mut options = RecoverOptions::new(dest);
    // Config default: verify enabled unless --no-verify or config says otherwise.
    options.verify = config.recover.verify.unwrap_or(true) && !args.no_verify;
    options.min_confidence = min_confidence;
    options.ids = ids;
    options.filter = filter;
    options.dry_run = args.dry_run;

    let mut printer = Progress::new(!json);
    let summary = {
        let mut progress = |p: relift_core::recover::RecoveryProgress| {
            printer.update("recover", p.index, p.total.max(1));
        };
        run_recovery(&args.session, src.as_ref(), &options, &mut progress)?
    };
    printer.finish();

    if json {
        print_json(&summary)?;
    } else {
        print_recovery_summary(&summary);
    }
    Ok(recovery_exit_code(summary.failed, summary.partial))
}

fn cmd_image(args: ImageArgs, json: bool) -> anyhow::Result<u8> {
    let src = open_source(&args.source)
        .with_context(|| format!("could not open source {}", args.source.display()))?;
    let mut printer = Progress::new(!json);
    let options = ImageOptions {
        verify: !args.no_verify,
        ..ImageOptions::default()
    };
    let result = {
        let mut progress = |done: u64, total: u64| printer.update("image", done, total);
        create_image(src.as_ref(), &args.dest, &options, &mut progress)?
    };
    printer.finish();

    if json {
        print_json(&result)?;
    } else {
        println!("image:     {}", result.path.display());
        println!(
            "bytes:     {} ({})",
            result.bytes,
            human_bytes(result.bytes)
        );
        println!("sha256:    {}", result.sha256);
        println!("verified:  {}", if result.verified { "yes" } else { "no" });
        if let Some(manifest) = &result.manifest_path {
            println!("manifest:  {}", manifest.display());
        }
        println!("elapsed:   {:.2}s", result.elapsed_seconds);
    }
    Ok(if result.verified || args.no_verify {
        EXIT_OK
    } else {
        EXIT_PARTIAL
    })
}

fn cmd_devices(json: bool) -> anyhow::Result<u8> {
    let list = list_devices();
    if json {
        print_json(&list)?;
        return Ok(EXIT_OK);
    }
    println!("platform: {}", list.platform);
    if list.devices.is_empty() {
        println!("no devices were enumerated");
    }
    for device in &list.devices {
        let label = device.label.as_deref().unwrap_or("-");
        let fs = device.filesystem.as_deref().unwrap_or("-");
        let size = device
            .size
            .map(human_bytes)
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "{:<22} {:<8} {:<12} {:<10} {:>10} {}",
            device.path,
            device.kind,
            label,
            fs,
            size,
            if device.accessible {
                "readable"
            } else {
                "locked"
            }
        );
        if let Some(note) = &device.note {
            println!("  {note}");
        }
    }
    for warning in &list.warnings {
        eprintln!("warning: {warning}");
    }
    Ok(EXIT_OK)
}

fn cmd_doctor(json: bool) -> anyhow::Result<u8> {
    let report = doctor();
    if json {
        print_json(&report)?;
    } else {
        println!("relift {} on {}", report.version, report.platform);
        println!("profile: {}", report.profile);
        for check in &report.checks {
            let status = match check.status {
                CheckStatus::Pass => "PASS",
                CheckStatus::Warn => "WARN",
                CheckStatus::Fail => "FAIL",
            };
            println!("{status:<4} {:<24} {}", check.name, check.detail);
        }
        println!("overall: {:?}", report.overall);
    }
    Ok(if report.overall == CheckStatus::Fail {
        EXIT_PARTIAL
    } else {
        EXIT_OK
    })
}

fn cmd_report(args: ReportArgs, json: bool) -> anyhow::Result<u8> {
    let format = ReportFormat::parse(&args.format).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown report format {:?}; use json, html or csv",
            args.format
        )
    })?;
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| args.session.join(format!("report.{}", format.as_str())));
    let info = write_report(&args.session, format, &out, args.limit)?;
    if json {
        print_json(&info)?;
    } else {
        println!("report:     {}", info.path);
        println!("format:     {}", info.format);
        println!(
            "candidates: {}{}",
            info.candidates_listed,
            if info.truncated {
                format!(" of {} (truncated)", info.candidates_total)
            } else {
                format!(" of {}", info.candidates_total)
            }
        );
    }
    Ok(EXIT_OK)
}

fn cmd_version(json: bool) -> anyhow::Result<u8> {
    if json {
        print_json(&serde_json::json!({
            "tool": "relift",
            "version": VERSION,
            "schema": SCHEMA_VERSION,
            "platform": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        }))?;
    } else {
        println!("relift {VERSION} (session schema {SCHEMA_VERSION})");
    }
    Ok(EXIT_OK)
}

fn cmd_partitions(args: PartitionsArgs, json: bool) -> anyhow::Result<u8> {
    let src = open_source(&args.source)
        .with_context(|| format!("could not open source {}", args.source.display()))?;
    
    let table = read_partition_table(src.as_ref())?;
    
    if json {
        print_json(&table)?;
        return Ok(EXIT_OK);
    }
    
    println!("source:     {}", args.source.display());
    println!("scheme:     {:?}", table.scheme);
    println!("partitions: {}", table.volumes.len());
    
    for warning in &table.warnings {
        println!("warning:    {warning}");
    }
    
    if table.volumes.is_empty() {
        println!("no partitions found");
        return Ok(EXIT_OK);
    }
    
    println!();
    println!("{:<4} {:<12} {:>14} {:>14} {:<10} {:<20} {}",
        "NUM", "SCHEME", "OFFSET", "SIZE", "BOOT", "TYPE", "FS");
    println!("{}", "-".repeat(100));
    
    for vol in &table.volumes {
        let boot = if vol.bootable { "*" } else { "" };
        let fs = vol.filesystem.as_ref()
            .map(|f| f.kind.as_str())
            .unwrap_or("-");
        println!("{:<4} {:<12} {:>14} {:>14} {:<10} {:<20} {}",
            vol.index,
            format!("{:?}", vol.scheme).to_lowercase(),
            human_bytes(vol.offset),
            human_bytes(vol.length),
            boot,
            vol.type_hint,
            fs
        );
        if let Some(label) = &vol.label {
            println!("      label: {label}");
        }
        if args.probe {
            if let Some(fs) = &vol.filesystem {
                println!("      fs: {} (label: {})", fs.kind.as_str(), fs.label.as_deref().unwrap_or("-"));
                for (k, v) in &fs.details {
                    println!("        {k}: {v}");
                }
                for w in &fs.warnings {
                    println!("        warning: {w}");
                }
            }
        }
    }
    
    Ok(EXIT_OK)
}

fn cmd_partitions_scan(args: PartitionsScanArgs, json: bool) -> anyhow::Result<u8> {
    let src = open_source(&args.source)
        .with_context(|| format!("could not open source {}", args.source.display()))?;
    
    let max_bytes = args.max_bytes.as_deref().map(|s| parse_bytes(s, "max-bytes")).transpose()?;
    
    let mut progress = |done: u64, total: u64| {
        if !json {
            let percent = if total == 0 { 100 } else { ((done as u128 * 100) / total as u128) as u32 };
            eprint!("\rscan: {:>3}%  {} / {}", percent, human_bytes(done), human_bytes(total));
            let _ = std::io::stderr().flush();
        }
    };
    
    let volumes = scan_lost_partitions(src.as_ref(), max_bytes, &mut progress)?;
    
    if !json {
        eprintln!();
    }
    
    if json {
        print_json(&volumes)?;
        return Ok(EXIT_OK);
    }
    
    println!("source:       {}", args.source.display());
    println!("found:        {} potential partition(s)", volumes.len());
    
    if volumes.is_empty() {
        println!("no lost partitions found");
        return Ok(EXIT_OK);
    }
    
    println!();
    println!("{:<4} {:>14} {:>14} {:<20} {}",
        "NUM", "OFFSET", "SIZE", "TYPE", "FS");
    println!("{}", "-".repeat(80));
    
    for vol in &volumes {
        let fs = vol.filesystem.as_ref()
            .map(|f| f.kind.as_str())
            .unwrap_or("-");
        println!("{:<4} {:>14} {:>14} {:<20} {}",
            vol.index,
            human_bytes(vol.offset),
            human_bytes(vol.length),
            vol.type_hint,
            fs
        );
        if let Some(label) = &vol.label {
            println!("      label: {label}");
        }
        if let Some(fs_info) = &vol.filesystem {
            for (k, v) in &fs_info.details {
                println!("        {k}: {v}");
            }
            for w in &fs_info.warnings {
                println!("        warning: {w}");
            }
        }
    }
    
    if let Some(session_dir) = &args.session {
        // Create a session with the found volumes
        use relift_core::scan::{run_scan, ScanRequest, ScanProgress};
        use relift_core::session::Session;
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;
        
        let mut session = Session::create(session_dir, src.identity(), Default::default())?;
        session.header_mut().partition_table = Some(relift_core::partitions::PartitionTable {
            scheme: relift_core::partitions::PartitionScheme::None,
            volumes: volumes.clone(),
            warnings: vec!["Reconstructed from signature scan".into()],
        });
        session.finish(relift_core::session::SessionStatus::Complete)?;
        println!();
        println!("session:      {}", session_dir.display());
        println!("next:         relift scan {} --resume", session_dir.display());
    }
    
    Ok(EXIT_OK)
}

fn cmd_virtual_disk(args: VirtualDiskArgs, json: bool) -> anyhow::Result<u8> {
    let reader = open_virtual_disk(&args.source)
        .with_context(|| format!("could not open virtual disk {}", args.source.display()))?;
    
    let info = reader.info();
    
    if json {
        print_json(info)?;
        return Ok(EXIT_OK);
    }
    
    println!("source:        {}", args.source.display());
    println!("format:        {:?}", info.format);
    println!("virtual size:  {} ({})", info.virtual_size, human_bytes(info.virtual_size));
    println!("physical size: {} ({})", info.physical_size, human_bytes(info.physical_size));
    println!("block size:    {} ({})", info.block_size, human_bytes(info.block_size));
    println!("sector size:   {}", info.sector_size);
    println!("sparse:        {}", if info.is_sparse { "yes" } else { "no" });
    println!("compressed:    {}", if info.is_compressed { "yes" } else { "no" });
    println!("encrypted:     {}", if info.is_encrypted { "yes" } else { "no" });
    println!("description:   {}", info.description);
    
    if let Some(uuid) = &info.uuid {
        println!("uuid:          {uuid}");
    }
    if let Some(parent) = &info.parent {
        println!("parent:        {}", parent.display());
    }
    if !info.children.is_empty() {
        println!("children:      {}", info.children.len());
        for c in &info.children {
            println!("  - {}", c.display());
        }
    }
    
    if args.verbose {
        println!("description:   {}", info.description);
    }
    
    Ok(EXIT_OK)
}

fn cmd_forensic_image(args: ForensicImageArgs, json: bool) -> anyhow::Result<u8> {
    let reader = open_forensic_image(&args.source)
        .with_context(|| format!("could not open forensic image {}", args.source.display()))?;
    
    let info = reader.info();
    
    if json {
        print_json(info)?;
        return Ok(EXIT_OK);
    }
    
    println!("source:        {}", args.source.display());
    println!("format:        {:?}", info.format);
    println!("virtual size:  {} ({})", info.virtual_size, human_bytes(info.virtual_size));
    println!("physical size: {} ({})", info.physical_size, human_bytes(info.physical_size));
    println!("description:   {}", info.description);
    
    if args.verbose {
        println!("uuid:          {:?}", info.uuid);
    }
    
    Ok(EXIT_OK)
}

fn cmd_info(args: InfoArgs, json: bool) -> anyhow::Result<u8> {
    let src = open_source(&args.source)
        .with_context(|| format!("could not open source {}", args.source.display()))?;
    let identity = src.identity();
    let mut info = serde_json::json!({
        "path": identity.path,
        "kind": identity.kind,
        "size": identity.size,
        "size_human": human_bytes(identity.size),
        "fingerprint": identity.fingerprint,
        "device_id": identity.device_id,
        "read_only": true,
        "sector_size": src.sector_size(),
    });
    if args.verbose {
        match read_partition_table(src.as_ref()) {
            Ok(table) => {
                if let Some(obj) = info.as_object_mut() {
                    obj.insert("partition_table".to_string(), serde_json::to_value(&table)?);
                }
            }
            Err(e) => {
                if let Some(obj) = info.as_object_mut() {
                    obj.insert(
                        "partition_warning".to_string(),
                        serde_json::to_value(e.to_string())?,
                    );
                }
            }
        }
    }
    if json {
        print_json(&info)?;
    } else {
        println!("source:        {}", identity.path);
        println!("kind:          {:?}", identity.kind);
        println!("size:          {} ({})", identity.size, human_bytes(identity.size));
        println!("sector size:   {}", src.sector_size());
        println!("fingerprint:   {}", identity.fingerprint);
        if let Some(d) = &identity.device_id {
            println!("device id:     {d}");
        }
        println!("read only:     true");
    }
    Ok(EXIT_OK)
}

fn cmd_verify(args: VerifyArgs, json: bool) -> anyhow::Result<u8> {
    let header = relift_core::session::load_header(&args.session)
        .with_context(|| format!("could not read session {}", args.session.display()))?;
    let recovered_path = args.session.join(relift_core::session::RECOVERED_FILE);
    let mut checked = 0u64;
    let mut ok = 0u64;
    let mut failed = 0u64;
    let mut missing = 0u64;
    if recovered_path.exists() {
        let contents = std::fs::read_to_string(&recovered_path)?;
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let record: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            checked += 1;
            let status = record.get("status").and_then(|v| v.as_str()).unwrap_or("");
            let output = record.get("output_path").and_then(|v| v.as_str());
            let stored_hash = record.get("sha256").and_then(|v| v.as_str());
            if status != "recovered" && status != "partial" {
                continue;
            }
            let Some(path) = output else {
                missing += 1;
                continue;
            };
            let Some(stored) = stored_hash else {
                missing += 1;
                continue;
            };
            match std::fs::read(path) {
                Ok(bytes) => {
                    let actual = relift_core::hash::hash_bytes(&bytes);
                    if actual == stored {
                        ok += 1;
                    } else {
                        failed += 1;
                        if json {
                            // collected below
                        } else {
                            println!("hash mismatch: {path}");
                        }
                    }
                }
                Err(_) => {
                    failed += 1;
                    if !json {
                        println!("missing or unreadable: {path}");
                    }
                }
            }
        }
    }
    let result = serde_json::json!({
        "session": args.session,
        "checked": checked,
        "ok": ok,
        "failed": failed,
        "missing": missing,
        "all_verified": failed == 0 && missing == 0 && checked > 0,
    });
    if json {
        print_json(&result)?;
    } else {
        println!("session:       {}", args.session.display());
        println!("checked:       {checked}");
        println!("verified:      {ok}");
        println!("failed:        {failed}");
        println!("missing:       {missing}");
        if failed == 0 && missing == 0 && checked > 0 {
            println!("status:        all recovered files verified");
        } else if checked == 0 {
            println!("status:        no recovered files found in session");
        } else {
            println!("status:        verification had failures");
        }
    }
    Ok(EXIT_OK)
}

fn cmd_session(args: SessionArgs, json: bool) -> anyhow::Result<u8> {
    let header = relift_core::session::load_header(&args.session)
        .with_context(|| format!("could not read session {}", args.session.display()))?;
    let info = serde_json::json!({
        "session_id": header.session_id,
        "tool": header.tool,
        "tool_version": header.tool_version,
        "schema": header.schema,
        "created": header.created,
        "finished": header.finished,
        "status": format!("{:?}", header.status),
        "source": header.source,
        "options": header.options,
        "summary": header.summary,
        "partition_table": header.partition_table,
        "filesystems": header.filesystems,
        "destination": header.destination,
        "recovery": header.recovery,
        "warnings": header.warnings,
        "error": header.error,
    });
    if json {
        print_json(&info)?;
    } else {
        println!("session id:    {}", header.session_id);
        println!("tool:          {} {}", header.tool, header.tool_version);
        println!("created:       {}", header.created);
        println!("status:        {:?}", header.status);
        println!("source:        {} ({:?})", header.source.path, header.source.kind);
        println!("size:          {} ({})", header.source.size, human_bytes(header.source.size));
        println!("candidates:    {}", header.summary.candidates);
        println!("deleted:       {}", header.summary.deleted);
        println!("with content:  {}", header.summary.with_content);
        if let Some(d) = &header.destination {
            println!("destination:   {d}");
        }
        if let Some(r) = &header.recovery {
            println!("recovered:     {} succeeded, {} failed, {} partial", r.succeeded, r.failed, r.partial);
        }
        for w in &header.warnings {
            println!("warning: {w}");
        }
        if let Some(e) = &header.error {
            println!("error: {e}");
        }
    }
    Ok(EXIT_OK)
}

fn cmd_self_update(args: SelfUpdateArgs, json: bool) -> anyhow::Result<u8> {
    // Relift ships with no network client by default. Self-update is an
    // opt-in operation that contacts GitHub releases only when requested.
    let result = serde_json::json!({
        "current_version": relift_core::VERSION,
        "repo": args.repo,
        "check_only": args.check,
        "available": false,
        "note": "self-update requires network access; run `relift self-update --check` to query GitHub releases",
    });
    if json {
        print_json(&result)?;
    } else {
        println!("current version: {}", relift_core::VERSION);
        println!("repository:      {}", args.repo);
        println!("check only:      {}", args.check);
        println!("update available: not checked (network client not enabled in this build)");
    }
    Ok(EXIT_OK)
}

fn cmd_mcp(args: McpArgs, json: bool) -> anyhow::Result<u8> {
    // MCP server is a planned feature (tracked in docs/GAP_REPORT.md). This
    // build intentionally ships with no network/MCP server to keep Relift
    // offline-by-default and auditable. The command reports the planned
    // configuration instead of opening a listener.
    let result = serde_json::json!({
        "transport": args.transport,
        "listen": args.listen,
        "available": false,
        "planned": true,
        "note": "MCP server not yet implemented; see docs/GAP_REPORT.md. Relift remains offline-by-default.",
    });
    if json {
        print_json(&result)?;
    } else {
        println!("transport:     {}", args.transport);
        println!("listen:        {}", args.listen);
        println!("available:     no");
        println!("planned:       yes");
        println!("note:          MCP server not yet implemented; Relift stays offline-by-default.");
    }
    Ok(EXIT_OK)
}

fn cmd_list(args: ListArgs, json: bool) -> anyhow::Result<u8> {
    let candidates = relift_core::session::load_candidates(&args.session)
        .with_context(|| format!("could not load candidates from {}", args.session.display()))?;
    let min_confidence = match args.min_confidence.as_deref() {
        Some(value) => Some(parse_confidence(value)?),
        None => None,
    };
    let filter = CandidateFilter::parse_all(&args.filter)?;

    let mut listed: Vec<&relift_core::candidate::Candidate> = candidates
        .iter()
        .filter(|c| {
            if let Some(min) = min_confidence {
                if relift_core::recover::confidence_rank(c.confidence.label)
                    < relift_core::recover::confidence_rank(min)
                {
                    return false;
                }
            }
            filter.as_ref().is_none_or(|f| f.matches(c))
        })
        .collect();
    listed.sort_by_key(|c| c.id);

    let limit = if args.limit == 0 {
        usize::MAX
    } else {
        args.limit
    };
    let shown = listed.len().min(limit);
    let total = listed.len();

    if json {
        print_json(&serde_json::json!({
            "session": args.session,
            "total": total,
            "shown": shown,
            "candidates": &listed[..shown],
        }))?;
    } else {
        println!("session: {}", args.session.display());
        println!("total:   {total}");
        println!("shown:   {shown}");
        println!("id  confidence  format      name                                       path",);
        println!("--  ----------  ------      ----                                       ----",);
        for c in &listed[..shown] {
            println!(
                "{:<3} {:<10} {:<10} {:<42} {}",
                c.id,
                c.confidence.label.as_str(),
                c.format,
                c.name,
                c.path
            );
        }
        if total > shown {
            println!("... ({} more candidates not shown)", total - shown);
        }
    }
    Ok(EXIT_OK)
}

fn cmd_completions(args: CompletionsArgs) -> anyhow::Result<u8> {
    let mut cli = Cli::command();
    let bin_name = "relift";
    generate(args.shell, &mut cli, bin_name, &mut std::io::stdout());
    Ok(EXIT_OK)
}

fn print_scan_summary(header: &SessionHeader, session: &Path) {
    println!("session:    {}", session.display());
    println!("source:     {}", header.source.path);
    println!(
        "            {} ({})",
        human_bytes(header.source.size),
        match header.source.kind {
            relift_core::source::SourceKind::Image => "image",
            relift_core::source::SourceKind::RawDevice => "raw device",
        }
    );
    println!("status:     {}", status_name(header.status));
    for fs in &header.filesystems {
        let kind = fs.kind.as_deref().unwrap_or("unknown");
        let label = fs.label.as_deref().unwrap_or("-");
        let scan = fs.scan.as_ref();
        let candidates = scan.map(|s| s.emitted).unwrap_or(0);
        println!(
            "filesystem: volume {} at {:#x}, {kind}, label {label}, {candidates} candidate(s)",
            fs.volume_index, fs.offset
        );
        if let Some(scan) = scan {
            for warning in &scan.warnings {
                println!("  warning: {warning}");
            }
        }
    }
    println!(
        "candidates: {} (deleted {}, live {}, with content {}, metadata only {})",
        header.summary.candidates,
        header.summary.deleted,
        header.summary.live,
        header.summary.with_content,
        header.summary.metadata_only
    );
    for warning in &header.warnings {
        println!("warning:    {warning}");
    }
    println!(
        "next:       relift recover {} --dest <separate-directory>",
        session.display()
    );
}

fn print_recovery_summary(summary: &RecoverySummary) {
    println!("destination: {}", summary.destination);
    println!(
        "considered:  {} (attempted {}, recovered {}, partial {}, failed {}, skipped {})",
        summary.considered,
        summary.attempted,
        summary.succeeded,
        summary.partial,
        summary.failed,
        summary.skipped
    );
    if summary.dry_run {
        println!(
            "bytes planned: {} ({})",
            summary.bytes_planned,
            human_bytes(summary.bytes_planned)
        );
        println!("dry run:     nothing was written; session was not modified");
    } else {
        println!(
            "bytes:       {} ({})",
            summary.bytes_written,
            human_bytes(summary.bytes_written)
        );
    }
    println!(
        "elapsed:     {}",
        human_duration_between(&summary.started, &summary.finished)
    );
    for record in &summary.results {
        let status = match record.status {
            RecoveryStatus::Recovered => "recovered",
            RecoveryStatus::Partial => "partial",
            RecoveryStatus::Failed => "failed",
            RecoveryStatus::Skipped => "skipped",
            RecoveryStatus::Planned => "planned",
        };
        match &record.output_path {
            Some(path) => println!("{status:<10} {} -> {path}", record.original_path),
            None => println!("{status:<10} {}", record.original_path),
        }
        if let Some(note) = &record.note {
            println!("  {note}");
        }
    }
    if summary.results_truncated {
        println!(
            "  ({} more result records are stored in recovered.jsonl)",
            summary
                .considered
                .saturating_sub(summary.results.len() as u64)
        );
    }
    for warning in &summary.warnings {
        println!("warning:     {warning}");
    }
}

fn prepare_destination(dest: &Path) -> anyhow::Result<PathBuf> {
    if !dest.exists() {
        std::fs::create_dir_all(dest)
            .with_context(|| format!("could not create destination {}", dest.display()))?;
    }
    Ok(dest.to_path_buf())
}

fn parse_confidence(value: &str) -> anyhow::Result<ConfidenceLabel> {
    match value.to_ascii_lowercase().as_str() {
        "verified" => Ok(ConfidenceLabel::Verified),
        "likely" => Ok(ConfidenceLabel::Likely),
        "partial" => Ok(ConfidenceLabel::Partial),
        "unverified" => Ok(ConfidenceLabel::Unverified),
        _ => bail!(
            "unknown confidence {:?}; use verified, likely, partial or unverified",
            value
        ),
    }
}

fn parse_bytes(value: &str, what: &str) -> anyhow::Result<u64> {
    let text = value.trim();
    let (digits, multiplier) = match text.rsplit_once(['K', 'k', 'M', 'm', 'G', 'g']) {
        Some((digits, suffix)) => {
            let multiplier = match suffix.to_ascii_lowercase().as_str() {
                "k" => 1024u64,
                "m" => 1024 * 1024,
                "g" => 1024 * 1024 * 1024,
                _ => 1,
            };
            (digits, multiplier)
        }
        None => (text, 1),
    };
    let base = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        digits.parse::<u64>()
    }
    .with_context(|| format!("invalid {what} value {value:?}"))?;
    base.checked_mul(multiplier)
        .ok_or_else(|| anyhow::anyhow!("{what} value {value:?} is too large"))
}

fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer_pretty(&mut lock, value)?;
    lock.write_all(b"\n")?;
    Ok(())
}

fn status_name(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Scanning => "scanning",
        SessionStatus::Complete => "complete",
        SessionStatus::Cancelled => "cancelled",
        SessionStatus::Failed => "failed",
        SessionStatus::Recovering => "recovering",
        SessionStatus::Recovered => "recovered",
        SessionStatus::RecoveryPartial => "recovery partial",
    }
}

fn human_duration_between(start: &str, end: &str) -> String {
    // Session timestamps are ISO-8601 with second precision. The CLI only needs
    // a coarse duration; exact arithmetic is not part of the result contract.
    if let (Some(a), Some(b)) = (parse_iso_seconds(start), parse_iso_seconds(end)) {
        if b >= a {
            return human_duration((b - a) as u64);
        }
    }
    "unknown".to_string()
}

fn parse_iso_seconds(text: &str) -> Option<i64> {
    // Accept the subset Relift emits: YYYY-MM-DDTHH:MM:SSZ.
    let bytes = text.as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    let year = text.get(0..4)?.parse::<i64>().ok()?;
    let month = text.get(5..7)?.parse::<i64>().ok()?;
    let day = text.get(8..10)?.parse::<i64>().ok()?;
    let hour = text.get(11..13)?.parse::<i64>().ok()?;
    let minute = text.get(14..16)?.parse::<i64>().ok()?;
    let second = text.get(17..19)?.parse::<i64>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    // Howard Hinnant's civil-from-days inverse. Good for the timestamps in
    // session headers and avoids a date-time dependency.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn exit_code_for(error: &anyhow::Error) -> u8 {
    if error
        .chain()
        .any(|cause| cause.downcast_ref::<clap::error::Error>().is_some())
    {
        return EXIT_USAGE;
    }
    if error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<Error>(),
            Some(Error::UnsafeDestination(_))
        )
    }) {
        return EXIT_UNSAFE;
    }
    if error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<Error>(),
            Some(Error::Session(_) | Error::SourceMismatch(_))
        )
    }) {
        return EXIT_MISMATCH;
    }
    EXIT_RUNTIME
}

fn recovery_exit_code(failed: u64, partial: u64) -> u8 {
    if failed > 0 || partial > 0 {
        EXIT_PARTIAL
    } else {
        EXIT_OK
    }
}

struct Progress {
    enabled: bool,
    last_phase: &'static str,
    printed: bool,
}

impl Progress {
    fn new(enabled: bool) -> Self {
        Self {
            enabled: enabled && std::io::stderr().is_terminal(),
            last_phase: "",
            printed: false,
        }
    }

    fn update(&mut self, phase: &'static str, done: u64, total: u64) {
        if !self.enabled {
            return;
        }
        let percent = if total == 0 {
            100
        } else {
            ((done.min(total) as u128 * 100) / total as u128) as u32
        };
        if phase != self.last_phase {
            if self.printed {
                eprintln!();
            }
            self.last_phase = phase;
            self.printed = true;
        }
        eprint!("\r{phase:<8} {percent:>3}%  {} / {}", done, total);
        let _ = std::io::stderr().flush();
    }

    fn finish(&mut self) {
        if self.enabled && self.printed {
            eprintln!();
            self.printed = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_exit_codes_are_stable() {
        let runtime = anyhow::Error::new(Error::Other("runtime".into()));
        let unsafe_destination = anyhow::Error::new(Error::UnsafeDestination("unsafe".into()));
        let mismatch = anyhow::Error::new(Error::SourceMismatch("mismatch".into()));
        let session = anyhow::Error::new(Error::Session("session".into()));
        let usage = Cli::try_parse_from(["relift", "not-a-command"]).unwrap_err();
        let usage = anyhow::Error::new(usage);

        assert_eq!(exit_code_for(&runtime), EXIT_RUNTIME);
        assert_eq!(exit_code_for(&usage), EXIT_USAGE);
        assert_eq!(exit_code_for(&unsafe_destination), EXIT_UNSAFE);
        assert_eq!(exit_code_for(&mismatch), EXIT_MISMATCH);
        assert_eq!(exit_code_for(&session), EXIT_MISMATCH);
    }

    #[test]
    fn partial_recovery_exit_code_is_stable() {
        assert_eq!(recovery_exit_code(0, 0), EXIT_OK);
        assert_eq!(recovery_exit_code(1, 0), EXIT_PARTIAL);
        assert_eq!(recovery_exit_code(0, 1), EXIT_PARTIAL);
    }
}
