use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::source::hex;

/// Streaming SHA-256 helper.
#[derive(Debug)]
pub struct Hasher {
    inner: Sha256,
    bytes: u64,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            inner: Sha256::new(),
            bytes: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
        self.bytes = self.bytes.saturating_add(data.len() as u64);
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn finish_hex(self) -> String {
        hex(&self.inner.finalize())
    }
}

pub fn hash_bytes(data: &[u8]) -> String {
    let mut h = Hasher::new();
    h.update(data);
    h.finish_hex()
}

pub fn hash_file(path: impl AsRef<Path>) -> Result<String> {
    let mut f = File::open(path).map_err(|e| Error::Io {
        offset: 0,
        source: e,
    })?;
    let mut h = Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| Error::Io {
            offset: 0,
            source: e,
        })?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finish_hex())
}

/// Constant-time comparison for hex digests (avoids timing leaks in the rare
/// case a hash is compared against user-supplied evidence).
pub fn digest_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn io_err(e: io::Error) -> Error {
    Error::Io {
        offset: 0,
        source: e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sha256() {
        assert_eq!(
            hash_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(digest_eq("abc", "abc"));
        assert!(!digest_eq("abc", "abd"));
        assert!(!digest_eq("abc", "ab"));
    }
}
