//! Candidate filter expressions.
//!
//! Filters are intentionally small, explicit and documented. They are used by
//! `relift list` and `relift recover --filter`, so the same expression means the
//! same thing in both places.
//!
//! Supported fields:
//!
//! - `ext:jpg,png,pdf` — match the candidate format or filename extension.
//! - `format:jpeg` — match the recorded candidate format exactly (aliases are
//!   normalised, so `jpg` and `jpeg` are equivalent).
//! - `name:IMG_` — case-insensitive substring of the recovered filename.
//! - `path:DCIM` — case-insensitive substring of the recovered relative path.
//!
//! Multiple expressions are combined with AND; values inside one expression are
//! combined with OR.

use std::path::Path;

use crate::candidate::Candidate;
use crate::error::{Error, Result};

/// A parsed, immutable candidate filter.
#[derive(Debug, Clone, Default)]
pub struct CandidateFilter {
    extensions: Vec<String>,
    formats: Vec<String>,
    names: Vec<String>,
    paths: Vec<String>,
}

impl CandidateFilter {
    /// Parse zero or more expressions. An empty slice yields `Ok(None)` so
    /// callers can skip filtering entirely.
    pub fn parse_all(expressions: &[String]) -> Result<Option<Self>> {
        if expressions.is_empty() {
            return Ok(None);
        }
        let mut filter = Self::default();
        for expression in expressions {
            filter.add(expression)?;
        }
        Ok(Some(filter))
    }

    /// Parse one expression such as `ext:jpg,png`.
    pub fn parse(expression: &str) -> Result<Self> {
        let mut filter = Self::default();
        filter.add(expression)?;
        Ok(filter)
    }

    fn add(&mut self, expression: &str) -> Result<()> {
        let (field, value) = expression.split_once(':').ok_or_else(|| {
            Error::invalid(format!(
                "filter {expression:?} is missing a field; use ext:, format:, name: or path:"
            ))
        })?;
        let field = field.trim().to_ascii_lowercase();
        let value = value.trim();
        if value.is_empty() {
            return Err(Error::invalid(format!(
                "filter {expression:?} has an empty value"
            )));
        }
        match field.as_str() {
            "ext" => {
                for item in value.split(',') {
                    let item = normalize_extension(item);
                    if item.is_empty() {
                        return Err(Error::invalid(format!(
                            "filter {expression:?} contains an empty extension"
                        )));
                    }
                    self.extensions.push(item);
                }
            }
            "format" => {
                for item in value.split(',') {
                    let item = normalize_extension(item);
                    if item.is_empty() {
                        return Err(Error::invalid(format!(
                            "filter {expression:?} contains an empty format"
                        )));
                    }
                    self.formats.push(item);
                }
            }
            "name" => self.names.push(value.to_lowercase()),
            "path" => self.paths.push(value.to_lowercase()),
            _ => {
                return Err(Error::invalid(format!(
                    "unknown filter field {field:?}; supported fields are ext, format, name, path"
                )))
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
            && self.formats.is_empty()
            && self.names.is_empty()
            && self.paths.is_empty()
    }

    /// True when the candidate satisfies every populated field.
    pub fn matches(&self, candidate: &Candidate) -> bool {
        if !self.extensions.is_empty() && !self.extension_matches(candidate) {
            return false;
        }
        if !self.formats.is_empty() {
            let format = normalize_extension(&candidate.format);
            if !self
                .formats
                .iter()
                .any(|wanted| equivalent_extension(wanted, &format))
            {
                return false;
            }
        }
        if !self.names.is_empty() {
            let name = candidate.name.to_lowercase();
            if !self.names.iter().any(|needle| name.contains(needle)) {
                return false;
            }
        }
        if !self.paths.is_empty() {
            let path = candidate.display_path().to_lowercase();
            if !self.paths.iter().any(|needle| path.contains(needle)) {
                return false;
            }
        }
        true
    }

    fn extension_matches(&self, candidate: &Candidate) -> bool {
        let format = normalize_extension(&candidate.format);
        if self
            .extensions
            .iter()
            .any(|wanted| equivalent_extension(wanted, &format))
        {
            return true;
        }
        if let Some(extension) = Path::new(&candidate.name)
            .extension()
            .and_then(|value| value.to_str())
        {
            let extension = normalize_extension(extension);
            if self
                .extensions
                .iter()
                .any(|wanted| equivalent_extension(wanted, &extension))
            {
                return true;
            }
        }
        false
    }
}

fn normalize_extension(value: &str) -> String {
    value.trim().trim_start_matches('.').to_ascii_lowercase()
}

/// Common aliases used by filesystems and carvers. Both sides are normalised
/// before comparison so `ext:jpeg` matches a candidate recorded as `jpg`.
fn canonical_extension(value: &str) -> &str {
    match value {
        "jpeg" => "jpg",
        "tif" => "tiff",
        "htm" => "html",
        "heif" => "heic",
        "gzip" => "gz",
        "zst" => "zstd",
        "mpeg" | "mpg" => "mp4",
        "oga" => "ogg",
        "sqlite3" => "sqlite",
        "yaml" => "yml",
        other => other,
    }
}

fn equivalent_extension(a: &str, b: &str) -> bool {
    a == b || canonical_extension(a) == canonical_extension(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::{Confidence, DataLocation, RecoveryMethod};

    fn candidate(name: &str, format: &str, path: &str) -> Candidate {
        Candidate::new(
            name,
            10,
            format,
            DataLocation::none(),
            RecoveryMethod::Metadata,
            Confidence::verified(90, "test"),
            "fat32",
        )
        .with_path(path)
    }

    #[test]
    fn empty_expressions_disable_filtering() {
        assert!(CandidateFilter::parse_all(&[]).unwrap().is_none());
    }

    #[test]
    fn extension_filter_matches_format_and_name_and_aliases() {
        let filter = CandidateFilter::parse("ext:jpg,png").unwrap();
        assert!(filter.matches(&candidate("photo.JPEG", "jpeg", "dcim")));
        assert!(filter.matches(&candidate("noext", "jpg", "dcim")));
        assert!(filter.matches(&candidate("shot.png", "png", "dcim")));
        assert!(!filter.matches(&candidate("notes.txt", "txt", "dcim")));
    }

    #[test]
    fn name_and_path_filters_are_case_insensitive() {
        let filter =
            CandidateFilter::parse_all(&["name:img_".to_string(), "path:dcim".to_string()])
                .unwrap()
                .unwrap();
        assert!(filter.matches(&candidate("IMG_001.JPG", "jpg", "DCIM/2024")));
        assert!(!filter.matches(&candidate("IMG_001.JPG", "jpg", "other")));
        assert!(!filter.matches(&candidate("other.jpg", "jpg", "DCIM/2024")));
    }

    #[test]
    fn format_filter_uses_aliases() {
        let filter = CandidateFilter::parse("format:tif").unwrap();
        assert!(filter.matches(&candidate("scan.tiff", "tiff", "")));
        assert!(!filter.matches(&candidate("scan.png", "png", "")));
    }

    #[test]
    fn unknown_field_is_rejected() {
        let error = CandidateFilter::parse("size:>10").unwrap_err();
        assert!(error.to_string().contains("unknown filter field"));
    }

    #[test]
    fn missing_field_is_rejected() {
        let error = CandidateFilter::parse("jpg").unwrap_err();
        assert!(error.to_string().contains("missing a field"));
    }
}
