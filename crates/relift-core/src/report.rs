//! Session reports: JSON, self-contained HTML and CSV.
//!
//! Reports are generated from the session files, not from a live scan. The
//! candidate stream is read line by line, so a large session does not have to
//! fit in memory. Every value that came from a filesystem is escaped for the
//! target format.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::candidate::{Candidate, DataLocation};
use crate::error::{Error, Result};
use crate::session::{load_header, now_iso, CandidateReader, SessionHeader};
use crate::util::{human_bytes, human_duration};
use crate::VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Json,
    Html,
    Csv,
}

impl ReportFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "html" => Some(Self::Html),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Html => "html",
            Self::Csv => "csv",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportInfo {
    pub path: String,
    pub format: String,
    pub candidates_listed: u64,
    pub candidates_total: u64,
    pub truncated: bool,
}

pub fn write_report(
    session_dir: &Path,
    format: ReportFormat,
    out: &Path,
    limit: usize,
) -> Result<ReportInfo> {
    let header = load_header(session_dir)?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Io {
                offset: 0,
                source: e,
            })?;
        }
    }
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out)
        .map_err(|e| Error::Io {
            offset: 0,
            source: e,
        })?;
    let mut writer = BufWriter::new(file);
    let generated = now_iso();
    let mut listed = 0u64;
    let mut truncated = false;
    let mut reader = CandidateReader::open(session_dir)?;
    match format {
        ReportFormat::Json => {
            write_json(
                &mut writer,
                &header,
                &mut reader,
                limit,
                &generated,
                &mut listed,
                &mut truncated,
            )?;
        }
        ReportFormat::Html => {
            write_html(
                &mut writer,
                &header,
                &mut reader,
                limit,
                &generated,
                &mut listed,
                &mut truncated,
            )?;
        }
        ReportFormat::Csv => {
            write_csv(
                &mut writer,
                &header,
                &mut reader,
                limit,
                &mut listed,
                &mut truncated,
            )?;
        }
    }
    writer.flush().map_err(|e| Error::Io {
        offset: 0,
        source: e,
    })?;
    Ok(ReportInfo {
        path: out.to_string_lossy().to_string(),
        format: format.as_str().to_string(),
        candidates_listed: listed,
        candidates_total: header.summary.candidates,
        truncated,
    })
}

fn write_json(
    w: &mut BufWriter<File>,
    header: &SessionHeader,
    reader: &mut CandidateReader,
    limit: usize,
    generated: &str,
    listed: &mut u64,
    truncated: &mut bool,
) -> Result<()> {
    w.write_all(b"{\n  \"schema\": ")?;
    w.write_all(crate::SCHEMA_VERSION.to_string().as_bytes())?;
    w.write_all(b",\n  \"generated\": ")?;
    serde_json::to_writer(&mut *w, generated)?;
    w.write_all(b",\n  \"tool_version\": ")?;
    serde_json::to_writer(&mut *w, VERSION)?;
    w.write_all(b",\n  \"session\": ")?;
    serde_json::to_writer(&mut *w, header)?;
    w.write_all(b",\n  \"candidates\": [")?;
    let mut first = true;
    while let Some(c) = reader.next_candidate() {
        let c = c?;
        if limit > 0 && *listed >= limit as u64 {
            *truncated = true;
            break;
        }
        if !first {
            w.write_all(b",")?;
        }
        first = false;
        w.write_all(b"\n    ")?;
        // Re-serialize a parsed value so a tampered line cannot break the JSON
        // document or inject arbitrary markup into the report.
        let value: serde_json::Value = serde_json::to_value(&c)?;
        serde_json::to_writer(&mut *w, &value)?;
        *listed += 1;
    }
    w.write_all(b"\n  ],\n  \"candidates_listed\": ")?;
    w.write_all(listed.to_string().as_bytes())?;
    w.write_all(b",\n  \"truncated\": ")?;
    w.write_all(if *truncated { b"true" } else { b"false" })?;
    w.write_all(b"\n}\n")?;
    Ok(())
}

fn write_html(
    w: &mut BufWriter<File>,
    header: &SessionHeader,
    reader: &mut CandidateReader,
    limit: usize,
    generated: &str,
    listed: &mut u64,
    truncated: &mut bool,
) -> Result<()> {
    let title = format!("Relift recovery report — {}", header.session_id);
    write!(
        w,
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{}</title><style>{}</style></head><body>",
        esc(&title),
        CSS
    )?;
    write!(
        w,
        "<header><h1>Relift recovery report</h1>\
         <p class=\"muted\">Session {} · generated {} · Relift {} · no telemetry</p></header>",
        esc(&header.session_id),
        esc(generated),
        esc(VERSION)
    )?;
    write!(
        w,
        "<section class=\"cards\">\
         <div><span>candidates</span><strong>{}</strong></div>\
         <div><span>deleted entries</span><strong>{}</strong></div>\
         <div><span>with content</span><strong>{}</strong></div>\
         <div><span>metadata only</span><strong>{}</strong></div>\
         <div><span>status</span><strong>{}</strong></div></section>",
        header.summary.candidates,
        header.summary.deleted,
        header.summary.with_content,
        header.summary.metadata_only,
        esc(&format!("{:?}", header.status).to_ascii_lowercase())
    )?;
    write!(
        w,
        "<section><h2>Source</h2><table class=\"kv\">\
         <tr><th>Path</th><td>{}</td></tr>\
         <tr><th>Kind</th><td>{:?}</td></tr>\
         <tr><th>Size</th><td>{} ({} bytes)</td></tr>\
         <tr><th>Source fingerprint</th><td><code>{}</code></td></tr>\
         <tr><th>Created</th><td>{}</td></tr>\
         <tr><th>Finished</th><td>{}</td></tr></table></section>",
        esc(&header.source.path),
        header.source.kind,
        human_bytes(header.source.size),
        header.source.size,
        esc(&header.source.fingerprint),
        esc(&header.created),
        esc(header.finished.as_deref().unwrap_or("—"))
    )?;
    if let Some(table) = &header.partition_table {
        write!(
            w,
            "<section><h2>Partitions</h2><p>Scheme: {:?}</p><table><thead><tr>\
             <th>#</th><th>Offset</th><th>Length</th><th>Type</th><th>Label</th></tr></thead><tbody>",
            table.scheme
        )?;
        for v in &table.volumes {
            write!(
                w,
                "<tr><td>{}</td><td>{:#x}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                v.index,
                v.offset,
                human_bytes(v.length),
                esc(&v.type_hint),
                esc(v.label.as_deref().unwrap_or(""))
            )?;
        }
        write!(w, "</tbody></table></section>")?;
    }
    write!(
        w,
        "<section><h2>Filesystems</h2><table><thead><tr>\
         <th>Volume</th><th>Offset</th><th>Filesystem</th><th>Label</th>\
         <th>Deleted</th><th>Live</th><th>Emitted</th><th>Carved</th><th>Notes</th>\
         </tr></thead><tbody>"
    )?;
    for fs in &header.filesystems {
        let scan = fs.scan.as_ref();
        write!(
            w,
            "<tr><td>{}</td><td>{:#x}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
             <td>{}</td><td>{}</td><td>{}</td></tr>",
            fs.volume_index,
            fs.offset,
            esc(fs.kind.as_deref().unwrap_or("unknown")),
            esc(fs.label.as_deref().unwrap_or("")),
            scan.map(|s| s.deleted_entries).unwrap_or(0),
            scan.map(|s| s.live_entries).unwrap_or(0),
            scan.map(|s| s.emitted).unwrap_or(0),
            fs.carving.as_ref().map(|c| c.emitted).unwrap_or(0),
            esc(&scan.map(|s| s.warnings.join("; ")).unwrap_or_default())
        )?;
    }
    write!(w, "</tbody></table></section>")?;

    if !header.warnings.is_empty() {
        write!(w, "<section><h2>Warnings</h2><ul>")?;
        for warning in &header.warnings {
            write!(w, "<li>{}</li>", esc(warning))?;
        }
        write!(w, "</ul></section>")?;
    }

    write!(
        w,
        "<section><h2>Candidates</h2>\
         <p class=\"muted\">Confidence is a named evidence score, not a calibrated probability. \
         A carved item is structurally validated content, not a filesystem entry.</p>\
         <table><thead><tr><th>ID</th><th>Name</th><th>Original path</th><th>Size</th>\
         <th>Format</th><th>Method</th><th>Confidence</th><th>State</th><th>Complete</th>\
         <th>Location</th></tr></thead><tbody>"
    )?;
    while let Some(c) = reader.next_candidate() {
        let c = c?;
        if limit > 0 && *listed >= limit as u64 {
            *truncated = true;
            break;
        }
        write!(
            w,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
             <td title=\"{}\">{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            c.id,
            esc(&c.name),
            esc(&c.display_path()),
            human_bytes(c.size),
            esc(&c.format),
            esc(c.method.as_str()),
            esc(&c.confidence.reasons.join("; ")),
            esc(c.confidence.label.as_str()),
            esc(c.state.as_str()),
            if c.complete { "yes" } else { "no" },
            esc(&location_desc(&c.data))
        )?;
        *listed += 1;
    }
    write!(w, "</tbody></table>")?;
    if *truncated {
        write!(
            w,
            "<p class=\"warn\">List truncated at {} candidates. Generate JSON/CSV or raise the \
             limit for the full list.</p>",
            listed
        )?;
    }
    write!(
        w,
        "</section><footer>Generated offline by Relift {}. This report describes recovered \
         evidence; it does not claim that every deleted file was recoverable.</footer>\
         </body></html>\n",
        esc(VERSION)
    )?;
    Ok(())
}

fn write_csv(
    w: &mut BufWriter<File>,
    _header: &SessionHeader,
    reader: &mut CandidateReader,
    limit: usize,
    listed: &mut u64,
    truncated: &mut bool,
) -> Result<()> {
    w.write_all(
        b"id,name,path,size,format,category,method,confidence,score,state,complete,filesystem,\
volume_offset,record_id,created,modified,accessed,deleted,content_sha256,location\n",
    )?;
    while let Some(c) = reader.next_candidate() {
        let c = c?;
        if limit > 0 && *listed >= limit as u64 {
            *truncated = true;
            break;
        }
        let fields = [
            c.id.to_string(),
            c.name.clone(),
            c.display_path(),
            c.size.to_string(),
            c.format.clone(),
            c.category.as_str().to_string(),
            c.method.as_str().to_string(),
            c.confidence.label.as_str().to_string(),
            c.confidence.score.to_string(),
            c.state.as_str().to_string(),
            if c.complete { "yes" } else { "no" }.to_string(),
            c.filesystem.clone(),
            c.volume_offset.to_string(),
            c.record_id.clone().unwrap_or_default(),
            c.created.clone().unwrap_or_default(),
            c.modified.clone().unwrap_or_default(),
            c.accessed.clone().unwrap_or_default(),
            c.deleted.clone().unwrap_or_default(),
            c.content_sha256.clone().unwrap_or_default(),
            location_desc(&c.data),
        ];
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                w.write_all(b",")?;
            }
            write_csv_field(w, f)?;
        }
        w.write_all(b"\n")?;
        *listed += 1;
    }
    Ok(())
}

fn write_csv_field(w: &mut BufWriter<File>, value: &str) -> Result<()> {
    if value.chars().any(|c| matches!(c, ',' | '"' | '\n' | '\r')) {
        w.write_all(b"\"")?;
        w.write_all(value.replace('"', "\"\"").as_bytes())?;
        w.write_all(b"\"")?;
    } else {
        w.write_all(value.as_bytes())?;
    }
    Ok(())
}

fn location_desc(data: &DataLocation) -> String {
    match data {
        DataLocation::Extents { extents } => format!("extents:{}", extents.len()),
        DataLocation::Segments { segments } => format!("segments:{}", segments.len()),
        DataLocation::Resident { bytes } => format!("resident:{}", bytes.len()),
        DataLocation::Raw { offset, length } => format!("raw:{offset:#x}+{length:#x}"),
        DataLocation::None => "none".into(),
    }
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

const CSS: &str = "body{font-family:system-ui,-apple-system,Segoe UI,Roboto,sans-serif;\
margin:2rem auto;max-width:1200px;padding:0 1rem;color:#1d232b;background:#fff}\
h1{margin-bottom:.2rem}h2{margin-top:2rem;border-bottom:1px solid #d9dee5;padding-bottom:.3rem}\
table{border-collapse:collapse;width:100%;font-size:.9rem}\
th,td{border:1px solid #d9dee5;padding:.35rem .5rem;text-align:left;vertical-align:top}\
th{background:#f4f6f8}code{font-size:.85rem;word-break:break-all}\
.cards{display:flex;gap:1rem;flex-wrap:wrap;margin:1.5rem 0}\
.cards div{border:1px solid #d9dee5;border-radius:6px;padding:.7rem 1rem;min-width:9rem}\
.cards span{display:block;color:#5d6875;font-size:.8rem;text-transform:uppercase}\
.cards strong{font-size:1.4rem}.muted{color:#5d6875}.warn{color:#8a4b00}\
footer{margin-top:3rem;color:#5d6875;font-size:.85rem}";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::{Confidence, RecoveryMethod};
    use crate::fs::CandidateSink;
    use crate::session::{Session, SessionOptions, SessionStatus};
    use crate::source::{SourceIdentity, SourceKind};

    #[test]
    fn html_escapes_names_and_all_formats_are_written() {
        let base = std::env::temp_dir().join(format!("relift-report-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let session = base.join("session");
        let identity = SourceIdentity {
            kind: SourceKind::Image,
            path: "x.img".into(),
            device_id: None,
            size: 1,
            fingerprint: "c".repeat(64),
        };
        let mut s = Session::create(
            &session,
            identity,
            SessionOptions {
                include_live: false,
                carve: false,
                max_candidates: 10,
                volume_indices: vec![],
                manual_volume: None,
            },
        )
        .unwrap();
        let c = Candidate::new(
            "<script>alert(1)</script>.txt",
            3,
            "txt",
            DataLocation::resident(b"abc".to_vec()),
            RecoveryMethod::FatDirectory,
            Confidence::verified(90, "test"),
            "fat32",
        );
        CandidateSink::emit(&mut s, c).unwrap();
        s.finish(SessionStatus::Complete).unwrap();
        for fmt in [ReportFormat::Json, ReportFormat::Html, ReportFormat::Csv] {
            let out = base.join(format!("report.{}", fmt.as_str()));
            write_report(&session, fmt, &out, 0).unwrap();
            let text = std::fs::read_to_string(&out).unwrap();
            assert!(!text.is_empty());
            if fmt == ReportFormat::Html {
                assert!(!text.contains("<script>alert(1)</script>"));
                assert!(text.contains("&lt;script&gt;"));
            }
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
