pub mod engine;

/// Lookup using bundled databases embedded in the crate.
pub use engine::api::{LookupResult, Region, lookup, lookup_with_paths};
pub use engine::error::GeoEngineError;
