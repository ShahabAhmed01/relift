//! Relift recovery core.
//!
//! The public design rule of this crate is simple: recovery code can only ever
//! see a [`source::ReadOnlySource`]. That trait exposes no write operation, so a
//! scanner cannot accidentally modify the media it is examining. All writes go
//! through the explicit recovery / imaging modules, which validate their
//! destination first.

#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_debug_implementations)]

pub mod candidate;
pub mod carve;
pub mod devices;
pub mod error;
pub mod filter;
pub mod fs;
pub mod hash;
pub mod imaging;
pub mod partitions;
pub mod recover;
pub mod report;
pub mod safety;
pub mod scan;
pub mod session;
pub mod source;
pub mod util;
pub mod virtual_disk;

pub use candidate::{
    Candidate, Confidence, ConfidenceLabel, DataLocation, EntryState, Extent, RecoveryMethod,
    Segment,
};
pub use error::{Error, Result};
pub use partitions::{
    read_partition_table, repair_partition_table, scan_lost_partitions, PartitionScheme,
    PartitionTable, Volume,
};
pub use source::{FileSource, ReadOnlySource, SourceIdentity};
pub use virtual_disk::{
    open_forensic_image, open_virtual_disk, open_virtual_disk_chain, VirtualDiskFormat,
    VirtualDiskInfo, VirtualDiskReader,
};

/// Tool version recorded in sessions and reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Session / report schema version.
pub const SCHEMA_VERSION: u32 = 1;
