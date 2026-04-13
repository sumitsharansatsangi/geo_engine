use std::path::PathBuf;

use geo_engine::{
    GeoEngineError, InitializedGeoEngine, find_district_profile, init_path, load_district_profiles,
    lookup_address_details_with_subdistrict_path, lookup_with_subdistrict_path, reverse_geocoding,
    search,
};

fn db_paths() -> (PathBuf, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    (root.join("geo.db"), root.join("subdistrict.db"))
}

fn data_csv_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data.csv")
}

fn city_asset_paths() -> (PathBuf, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    (
        root.join("cities-0.0.1.fst"),
        root.join("cities-0.0.1.rkyv"),
    )
}

#[test]
fn lookup_with_subdistrict_path_allows_non_india_without_subdistrict_db() {
    let (country_db, _) = db_paths();

    let result = lookup_with_subdistrict_path(51.5074, -0.1278, &country_db, None)
        .expect("lookup should succeed for non-India point");

    assert_eq!(result.country.name, "United Kingdom");
    assert_eq!(result.country.iso2, "GB");
    assert!(result.state.is_none());
    assert!(result.district.is_none());
    assert!(result.subdistrict.is_none());
}

#[test]
fn lookup_with_subdistrict_path_returns_error_when_missing() {
    let (country_db, _) = db_paths();
    let missing_subdistrict_db =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("missing_subdistrict.db");

    let err =
        lookup_with_subdistrict_path(25.25, 87.04, &country_db, Some(&missing_subdistrict_db))
            .expect_err("lookup should fail when subdistrict db is missing for India point");

    match err {
        GeoEngineError::DistrictDatabaseUnavailable { path, .. } => {
            assert_eq!(path, missing_subdistrict_db);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn lookup_with_subdistrict_path_returns_country_not_found_for_invalid_point() {
    let (country_db, _) = db_paths();

    let err = lookup_with_subdistrict_path(999.0, 999.0, &country_db, None)
        .expect_err("lookup should fail for impossible coordinates");

    match err {
        GeoEngineError::CountryNotFound { lat, lon } => {
            assert_eq!(lat, 999.0);
            assert_eq!(lon, 999.0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn lookup_with_subdistrict_path_returns_india_admin_hierarchy() {
    let (country_db, subdistrict_db) = db_paths();

    let result = lookup_with_subdistrict_path(25.25, 87.04, &country_db, Some(&subdistrict_db))
        .expect("lookup should succeed for known India/Bihar point");

    assert_eq!(result.country.name, "India");
    assert_eq!(result.country.iso2, "IN");
    assert_eq!(
        result
            .state
            .expect("state should be present for India")
            .name,
        "Bihar"
    );
    assert_eq!(
        result
            .district
            .expect("district should be present for India")
            .name,
        "Bhagalpur"
    );
    assert_eq!(
        result
            .subdistrict
            .expect("subdistrict should be present for India")
            .name,
        "Sabour"
    );
    let demographics = result
        .demographics
        .expect("embedded demographics should be present after enrichment");
    assert_eq!(demographics.district_uni_code, "IN-BR-BGP");
    assert_eq!(demographics.major_religion, "Hinduism");
    assert!(demographics.languages.iter().any(|language| {
        language.name == "Angika" && language.usage_type == "primary" && language.code == "anp"
    }));
}

#[test]
fn search_subdistricts_by_name_returns_matching_hierarchy() {
    let (country_db, subdistrict_db) = db_paths();
    let (city_fst, city_rkyv) = city_asset_paths();

    init_path(&country_db, &subdistrict_db, &city_fst, &city_rkyv)
        .expect("init_path should succeed");

    let result = search("sabour").expect("search should succeed for known subdistrict");

    assert!(
        result.subdistricts.iter().any(|matched| {
            matched.subdistrict.name == "Sabour"
                && matched.district.name == "Bhagalpur"
                && matched.state.name == "Bihar"
        }),
        "expected Sabour, Bhagalpur, Bihar in search results"
    );
}

#[test]
fn search_cities_by_name_returns_matches() {
    let (country_db, subdistrict_db) = db_paths();
    let (city_fst, city_rkyv) = city_asset_paths();

    init_path(&country_db, &subdistrict_db, &city_fst, &city_rkyv)
        .expect("init_path should succeed");

    let result = search("london").expect("city search should succeed for known city");

    assert!(
        !result.cities.is_empty(),
        "expected at least one city match"
    );
    assert!(
        result
            .cities
            .iter()
            .any(|matched| matched.name.to_lowercase().contains("london")),
        "expected a city containing 'london' in search results"
    );
}

#[test]
fn search_places_by_name_combines_city_and_subdistrict_results() {
    let (country_db, subdistrict_db) = db_paths();
    let (city_fst, city_rkyv) = city_asset_paths();

    init_path(&country_db, &subdistrict_db, &city_fst, &city_rkyv)
        .expect("init_path should succeed");

    let result = search("sabour").expect("combined search should succeed");

    // Verify subdistricts results
    assert!(
        result.subdistricts.iter().any(|matched| {
            matched.subdistrict.name == "Sabour"
                && matched.district.name == "Bhagalpur"
                && matched.state.name == "Bihar"
        }),
        "expected Sabour, Bhagalpur, Bihar in combined search results"
    );

    // Verify cities results are populated (structure validation)
    // Note: May or may not have sabour city results, but structure should be valid
    let cities_valid = result
        .cities
        .iter()
        .all(|city| !city.name.is_empty() && !city.country_code.is_empty() && city.geoname_id > 0);
    assert!(cities_valid, "all city matches should have valid structure");
}

#[test]
fn reverse_geocoding_returns_country_and_city_for_non_india_point() {
    let (country_db, subdistrict_db) = db_paths();
    let (city_fst, city_rkyv) = city_asset_paths();

    // Keep read of fst path in scope to validate assets are present for search APIs too.
    assert!(city_fst.exists(), "city fst asset should exist");

    init_path(&country_db, &subdistrict_db, &city_fst, &city_rkyv)
        .expect("init_path should succeed");

    let result = reverse_geocoding(51.5074, -0.1278)
        .expect("reverse geocoding should succeed for non-India point");

    assert_eq!(result.country.name, "United Kingdom");
    assert!(result.state.is_none());
    assert!(result.district.is_none());
    assert!(result.subdistrict.is_none());
    assert!(!result.city.name.is_empty());
}

#[test]
fn reverse_geocoding_returns_subdistrict_and_city_for_india_point() {
    let (country_db, subdistrict_db) = db_paths();
    let (city_fst, city_rkyv) = city_asset_paths();

    init_path(&country_db, &subdistrict_db, &city_fst, &city_rkyv)
        .expect("init_path should succeed");

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
    assert!(!result.city.name.is_empty());
}

#[test]
fn search_by_name_combines_city_and_subdistrict_without_city_limit() {
    let (country_db, subdistrict_db) = db_paths();
    let (city_fst, city_rkyv) = city_asset_paths();

    init_path(&country_db, &subdistrict_db, &city_fst, &city_rkyv)
        .expect("init_path should succeed");

    let result = search("london").expect("unified search should succeed");

    assert!(
        result
            .cities
            .iter()
            .any(|city| city.name.to_lowercase().contains("london")),
        "expected london city results"
    );
}

#[test]
fn district_demographics_can_be_mapped_from_lookup_result() {
    let (country_db, subdistrict_db) = db_paths();
    let data_csv = data_csv_path();

    let result = lookup_with_subdistrict_path(25.25, 87.04, &country_db, Some(&subdistrict_db))
        .expect("lookup should succeed for known India/Bihar point");
    let district = result.district.expect("district should be present");

    let profiles = load_district_profiles(&data_csv).expect("data.csv should load");
    let profile = find_district_profile(&profiles, &district.iso2, &district.name)
        .expect("district profile should exist");

    assert_eq!(profile.district_name, "Bhagalpur");
    assert_eq!(profile.district_code, "IN-BR-BGP");
    assert_eq!(profile.major_religion, "Hinduism");
    assert!(profile.languages.iter().any(|language| {
        language.name == "Angika" && language.usage_type == "primary" && language.code == "anp"
    }));
    assert!(
        profile
            .languages
            .iter()
            .any(|language| language.name == "Hindi" && language.usage_type == "administrative")
    );
}

#[test]
fn lookup_address_details_returns_full_hierarchy_and_demographics() {
    let (country_db, subdistrict_db) = db_paths();

    let details = lookup_address_details_with_subdistrict_path(
        25.25,
        87.04,
        &country_db,
        Some(&subdistrict_db),
    )
    .expect("address details lookup should succeed");

    assert_eq!(details.full_address, "Sabour, Bhagalpur, Bihar, India");
    assert_eq!(details.country.name, "India");
    assert_eq!(
        details.state.as_ref().map(|region| region.name.as_str()),
        Some("Bihar")
    );
    assert_eq!(
        details.district.as_ref().map(|region| region.name.as_str()),
        Some("Bhagalpur")
    );
    assert_eq!(details.district_uni_code.as_deref(), Some("IN-BR-BGP"));
    assert_eq!(
        details
            .subdistrict
            .as_ref()
            .map(|region| region.name.as_str()),
        Some("Sabour")
    );
    assert_eq!(details.major_religion.as_deref(), Some("Hinduism"));
    assert!(details.languages.iter().any(|language| {
        language.name == "Angika" && language.usage_type == "primary" && language.code == "anp"
    }));
}

#[test]
fn lookup_address_details_returns_country_only_for_non_india_point() {
    let (country_db, _) = db_paths();

    let details = lookup_address_details_with_subdistrict_path(51.5074, -0.1278, &country_db, None)
        .expect("address details lookup should succeed for non-India point");

    assert_eq!(details.full_address, "United Kingdom");
    assert_eq!(details.country.iso2, "GB");
    assert!(details.state.is_none());
    assert!(details.district.is_none());
    assert!(details.subdistrict.is_none());
    assert!(details.district_uni_code.is_none());
    assert!(details.major_religion.is_none());
    assert!(details.languages.is_empty());
}

#[test]
fn initialized_engine_can_be_reused_for_multiple_lookups() {
    let (country_db, subdistrict_db) = db_paths();
    let (city_fst, city_rkyv) = city_asset_paths();

    let engine = InitializedGeoEngine::open(&country_db, &subdistrict_db, &city_fst, &city_rkyv)
        .expect("engine should initialize once");

    let india = engine
        .lookup_address_details(25.25, 87.04)
        .expect("india lookup should succeed");
    let non_india = engine
        .lookup_address_details(51.5074, -0.1278)
        .expect("non-india lookup should succeed");

    assert_eq!(india.full_address, "Sabour, Bhagalpur, Bihar, India");
    assert_eq!(india.major_religion.as_deref(), Some("Hinduism"));
    assert_eq!(non_india.full_address, "United Kingdom");
    assert!(non_india.languages.is_empty());
}

#[test]
fn lookup_result_includes_polygon_center_coordinates() {
    let (country_db, subdistrict_db) = db_paths();

    let result = lookup_with_subdistrict_path(25.25, 87.04, &country_db, Some(&subdistrict_db))
        .expect("lookup should succeed for known India/Bihar point");

    // The result should contain latitude and longitude from the subdistrict polygon center
    // Polygon centers can vary considerably from the input point
    assert!(
        result.latitude > 20.0 && result.latitude < 30.0,
        "latitude should be within expected India range, got {}",
        result.latitude
    );
    assert!(
        result.longitude > 80.0 && result.longitude < 95.0,
        "longitude should be within expected India range, got {}",
        result.longitude
    );
    // For non-India point
    let result_non_india = lookup_with_subdistrict_path(51.5074, -0.1278, &country_db, None)
        .expect("lookup should succeed for non-India point");

    // For non-India points without subdistrict, should return the input coordinates
    assert_eq!(result_non_india.latitude, 51.5074);
    assert_eq!(result_non_india.longitude, -0.1278);
}
