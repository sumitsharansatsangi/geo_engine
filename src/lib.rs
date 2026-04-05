pub mod engine;

/// Lookup using runtime-loaded databases.
pub use engine::api::{
    LookupResult, Region, init_databases, init_databases_from_strings, init_with_remote,
    init_with_remote_path, lookup, lookup_place, lookup_with_district_path, lookup_with_paths,
    lookup_with_subdistrict_path,
};
pub use engine::error::GeoEngineError;
