pub mod engine;

/// Lookup using runtime-loaded databases.
pub use engine::api::{LookupResult, Region, lookup_with_subdistrict_path};
pub use engine::error::GeoEngineError;
