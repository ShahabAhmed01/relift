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
use relift_core::recover::{run_recovery, RecoverOptions, RecoveryStatus, RecoverySummary};
use relift_core::report::{write_report, ReportFormat};
use relift_core::scan::{run_scan, ScanProgress, ScanRequest};
use relift_core::session::{load_header, ManualVolume, SessionHeader, SessionStatus};
use relift_core::source::open_source;
use relift_core::util::{human_bytes, human_duration};
use relift_core::{SCHEMA_VERSION, VERSION};
use serde::Serialize;

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

    /// New or empty directory for session output.
    #[arg(long, value_name = "DIR")]
    session: PathBuf,

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

    /// Output file. Defaults to <session>/report.<format>.
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,

    /// Maximum candidates to list (0 means all).
    #[arg(long, default_value_t = 0, value_name = "N")]
    limit: usize,
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
    match cli.command {
        Command::Scan(args) => cmd_scan(args, cli.json),
        Command::List(args) => cmd_list(args, cli.json),
        Command::Recover(args) => cmd_recover(args, cli.json),
        Command::Image(args) => cmd_image(args, cli.json),
        Command::Devices => cmd_devices(cli.json),
        Command::Doctor => cmd_doctor(cli.json),
        Command::Report(args) => cmd_report(args, cli.json),
        Command::Completions(args) => cmd_completions(args),
        Command::Version => cmd_version(cli.json),
    }
}

fn cmd_scan(args: ScanArgs, json: bool) -> anyhow::Result<u8> {
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
    request.include_live = args.live;
    request.carve = args.carve || args.mode == ScanMode::Deep;
    request.volume_indices = args.volume.clone();
    request.manual_volume = manual_volume;
    request.max_candidates = args.max_candidates.max(1);

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

fn cmd_recover(args: RecoverArgs, json: bool) -> anyhow::Result<u8> {
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
        None => None,
    };
    let ids = if args.id.is_empty() {
        None
    } else {
        Some(args.id.iter().copied().collect())
    };
    let filter = CandidateFilter::parse_all(&args.filter)?;
    let dest = prepare_destination(&args.dest)?;
    let mut options = RecoverOptions::new(dest);
    options.verify = !args.no_verify;
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
    Ok(if summary.failed > 0 || summary.partial > 0 {
        EXIT_PARTIAL
    } else {
        EXIT_OK
    })
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
