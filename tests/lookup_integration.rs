use std::path::PathBuf;

use geo_engine::{GeoEngineError, init_path, reverse_geocoding, search};

fn asset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn init_for_tests() {
    init_path(asset_dir().display().to_string(), true).expect("init_path should succeed");
}

#[test]
fn search_subdistrict_returns_expected_match() {
    init_for_tests();

    let result = search("sabour").expect("search should succeed");

    assert!(
        result.subdistricts.iter().any(|matched| {
            matched.subdistrict.name == "Sabour"
                && matched.district.name == "Bhagalpur"
                && matched.state.name == "Bihar"
        }),
        "expected Sabour, Bhagalpur, Bihar in results"
    );
}

#[test]
fn search_city_returns_london() {
    init_for_tests();

    let result = search("london").expect("search should succeed");

    assert!(
        result
            .cities
            .iter()
            .any(|city| city.name.to_lowercase().contains("london")),
        "expected at least one london city result"
    );
}

#[test]
fn reverse_geocoding_non_india_has_country_and_city() {
    init_for_tests();

    let result = reverse_geocoding(51.5074, -0.1278)
        .expect("reverse geocoding should succeed for non-India point");

    assert_eq!(result.country.name, "United Kingdom");
    assert!(result.state.is_none());
    assert!(result.district.is_none());
    assert!(result.subdistrict.is_none());
    assert!(!result.city.name.is_empty());
}

#[test]
fn reverse_geocoding_india_has_subdistrict() {
    init_for_tests();

    let result =
        reverse_geocoding(25.25, 87.04).expect("reverse geocoding should succeed for India point");

    assert_eq!(result.country.iso2, "IN");
    assert_eq!(
        result
            .subdistrict
            .as_ref()
            .map(|region| region.name.as_str()),
        Some("Sabour")
    );
}

#[test]
fn init_path_rejects_different_second_paths() {
    let root = asset_dir();

    init_path(root.display().to_string(), true).expect("first init should succeed");

    let different_asset_dir = root.join("other-assets");
    let err = init_path(different_asset_dir.display().to_string(), true)
        .expect_err("second init with different paths should fail");

    assert!(matches!(err, GeoEngineError::PathsAlreadyInitialized));
}

#[test]
fn init_path_accepts_equivalent_second_paths() {
    let root = asset_dir();

    init_path(root.display().to_string(), true).expect("first init should succeed");

    let equivalent_asset_dir = root.join(".");
    init_path(equivalent_asset_dir.display().to_string(), true)
        .expect("second init with equivalent path should succeed");
}
