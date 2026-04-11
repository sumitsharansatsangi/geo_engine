pub mod district_data;
pub mod engine;

/// Lookup using runtime-loaded databases.
pub use district_data::{
    DistrictProfile, GeoLanguage, find_district_profile, load_district_profiles,
};
pub use engine::api::{
    AddressDetails, DistrictDemographics, InitializedGeoEngine, LookupResult, Region,
    SubdistrictMatch, lookup_address_details_with_subdistrict_path, lookup_with_subdistrict_path,
    search_subdistricts_by_name,
};
pub use engine::error::GeoEngineError;
