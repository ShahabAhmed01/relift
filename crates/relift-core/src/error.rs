use std::io;

/// Errors produced by the recovery engine.
///
/// Offsets in error messages are physical offsets into the opened source, which
/// makes a failure reproducible against the same image or device.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error at offset {offset:#x}: {source}")]
    Io {
        offset: u64,
        #[source]
        source: io::Error,
    },

    #[error("I/O error: {0}")]
    PlainIo(#[from] io::Error),

    #[error("unexpected end of source at {offset:#x}: wanted {wanted} bytes, got {got}")]
    UnexpectedEof {
        offset: u64,
        wanted: usize,
        got: usize,
    },

    #[error("invalid source: {0}")]
    InvalidSource(String),

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("corrupt structure: {0}")]
    Corrupt(String),

    #[error("unsafe destination: {0}")]
    UnsafeDestination(String),

    #[error("session error: {0}")]
    Session(String),

    #[error("source mismatch: {0}")]
    SourceMismatch(String),

    #[error("scan cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(offset: u64, source: io::Error) -> Self {
        Self::Io { offset, source }
    }

    pub fn corrupt(msg: impl Into<String>) -> Self {
        Self::Corrupt(msg.into())
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::InvalidSource(msg.into())
    }

    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::Unsupported(msg.into())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Session(format!("JSON error: {e}"))
    }
}
