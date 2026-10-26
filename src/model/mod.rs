//! Canonical path based metadata and source scanning.
pub mod library;
pub mod scan;
pub mod store;
pub use library::*;
pub use scan::{ImageEntry, ScanResult, scan};
