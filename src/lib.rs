pub mod district_data;
pub mod engine;

/// Lookup using runtime-loaded databases.
pub use district_data::{
    DistrictLanguage, DistrictProfile, find_district_profile, load_district_profiles,
};
pub use engine::api::{
    AddressDetails, DistrictDemographics, InitializedGeoEngine, LookupResult, Region,
    SubdistrictMatch, default_engine, initialize_default_engine,
    lookup_address_details_with_default_engine, lookup_address_details_with_subdistrict_path,
    lookup_with_default_engine, lookup_with_subdistrict_path, search_subdistricts_by_name,
};
pub use engine::error::GeoEngineError;
