mod engine;

// Public surface intentionally kept minimal:
// - init_path
// - search
// - reverse_geocoding
//
// Other exports are intentionally commented out to avoid expanding
// the top-level API surface.
// pub use engine::bootstrap::{
//     AllAssetPaths, CityAssetPaths, InitConfig, init_all_assets, init_all_assets_in_background,
//     init_all_assets_in_background_with_config, init_all_assets_with_config, init_city_assets,
//     init_city_assets_with_config, init_geo_engine, init_geo_engine_with_config,
//     refresh_all_assets_in_background, refresh_all_assets_in_background_with_callback,
//     refresh_all_assets_in_background_with_callback_config,
//     refresh_all_assets_in_background_with_config, refresh_and_reopen_engine_in_background,
//     refresh_and_reopen_engine_in_background_with_config,
// };
// pub use engine::city::City;
// pub use engine::api::{
//     AddressDetails, CityMatch, CombinedSearchResult, DistrictDemographics, InitializedGeoEngine,
//     LookupResult, Region, ReverseGeocodingResult, SubdistrictMatch,
//     lookup_address_details_with_subdistrict_path, lookup_with_subdistrict_path,
// };
// pub use engine::error::GeoEngineError;

pub use engine::api::{
    CityMatch, CombinedSearchResult, Region, ReverseGeocodingResult, SubdistrictMatch,
};
pub use engine::api::{init_path, reverse_geocoding, search};
pub use engine::error::GeoEngineError;
