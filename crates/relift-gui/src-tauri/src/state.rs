//! Shared state management for Relift GUI.

use std::sync::{Arc, Mutex};

/// Global application state for managing scan and recovery operations.
#[derive(Default)]
pub struct AppState {
    /// Cancellation flag for scan operations.
    pub scan_cancel: Arc<Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>>,
    /// Cancellation flag for recovery operations.
    pub recover_cancel: Arc<Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>>,
}
