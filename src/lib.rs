mod engine;
pub mod frb_api;
pub mod spatial_sidecar;

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub mod wasm;

#[macro_export]
macro_rules! operation_failed {
    ($module:expr, $function:expr, $step:expr, $source:expr) => {
        $crate::engine::error::GeoEngineError::OperationFailed {
            operation: concat!($module, ".", $function, ".", $step),
            source: Box::new($source),
        }
    };
}

pub use engine::bootstrap::{
    AllAssetPaths, CityAssetPaths, init_all_assets, init_all_assets_in_background,
    init_all_assets_in_background_with_config, init_city_assets, init_city_assets_with_config,
    init_geo_engine, init_geo_engine_with_config, refresh_all_assets_in_background,
    refresh_all_assets_in_background_with_callback,
    refresh_all_assets_in_background_with_callback_config,
    refresh_all_assets_in_background_with_config, refresh_and_reopen_engine_in_background,
    refresh_and_reopen_engine_in_background_with_config,
};

pub use engine::api::{
    CityMatch, CombinedSearchResult, Region, ReverseGeocodingResult, SubdistrictMatch,
};
pub use engine::api::{
    InitializedGeoEngine, init_path, reverse_geocoding, reverse_geocoding_batch, search,
    search_batch,
};
pub use engine::error::GeoEngineError;
