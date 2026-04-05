use std::path::PathBuf;

use geo_engine::{GeoEngineError, lookup, lookup_place, lookup_with_district_path, lookup_with_paths};

fn db_paths() -> (PathBuf, PathBuf, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    (
        root.join("geo.db"),
        root.join("state_in.db"),
        root.join("district_in.db"),
    )
}

#[test]
fn lookup_with_paths_returns_country_and_state() {
    let (country_db, state_db, _) = db_paths();

    let result = lookup_with_paths(25.25, 87.04, &country_db, Some(&state_db))
        .expect("lookup should succeed for known India/Bihar point");

    assert_eq!(result.country.name, "India");
    assert_eq!(result.country.iso2, "IN");

    let state = result
        .state
        .expect("state should be present for India point");
    assert_eq!(state.name, "Bihar");
    assert!(result.district.is_none());
}

#[test]
fn lookup_with_district_path_returns_optional_district() {
    let (country_db, state_db, district_db) = db_paths();

    let result =
        lookup_with_district_path(25.25, 87.04, &country_db, Some(&state_db), Some(&district_db))
            .expect("lookup should succeed when district db is configured");

    assert_eq!(result.country.name, "India");
    assert_eq!(result.country.iso2, "IN");

    let state = result
        .state
        .expect("state should be present for India point");
    assert_eq!(state.name, "Bihar");
    let district = result
        .district
        .expect("district should be present when district db is configured");
    assert_eq!(district.name, "Bhagalpur");
    assert_eq!(district.iso2, "BH");
}

#[test]
fn lookup_with_paths_does_not_return_district_without_district_db() {
    let (country_db, state_db, _) = db_paths();

    let result = lookup_with_paths(25.25, 87.04, &country_db, Some(&state_db))
        .expect("lookup should succeed without district db");

    assert_eq!(result.country.name, "India");
    assert_eq!(
        result
            .state
            .expect("state should be present for India point")
            .name,
        "Bihar"
    );
    assert!(result.district.is_none());
}

#[test]
fn lookup_uses_bundled_databases() {
    let result = lookup(25.25, 87.04).expect("bundled lookup should succeed");
    assert_eq!(result.country.name, "India");

    let state = result
        .state
        .expect("state should be present for India point");
    assert_eq!(state.name, "Bihar");
    let district = result
        .district
        .expect("district should be present for bundled district db");
    assert_eq!(district.name, "Bhagalpur");
    assert_eq!(district.iso2, "BH");
}

#[test]
fn bundled_lookup_matches_explicit_district_lookup() {
    let (country_db, state_db, district_db) = db_paths();

    let bundled =
        lookup(25.25, 87.04).expect("bundled lookup should succeed for known district point");
    let explicit =
        lookup_with_district_path(25.25, 87.04, &country_db, Some(&state_db), Some(&district_db))
            .expect("explicit district lookup should succeed for known district point");

    assert_eq!(bundled.country, explicit.country);
    assert_eq!(bundled.state, explicit.state);
    assert_eq!(bundled.district, explicit.district);
}

#[test]
fn district_lookup_is_stable_for_nearby_point_in_same_district() {
    let (country_db, state_db, district_db) = db_paths();

    let result =
        lookup_with_district_path(25.30, 87.02, &country_db, Some(&state_db), Some(&district_db))
            .expect("lookup should succeed for nearby point in Bhagalpur");

    assert_eq!(result.country.name, "India");
    assert_eq!(
        result
            .state
            .expect("state should be present for India point")
            .name,
        "Bihar"
    );

    let district = result
        .district
        .expect("district should be present for nearby Bhagalpur point");
    assert_eq!(district.name, "Bhagalpur");
    assert_eq!(district.iso2, "BH");
}

#[test]
fn lookup_with_paths_returns_state_db_error_when_missing() {
    let (country_db, _, _) = db_paths();
    let missing_state_db = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("missing_state_in.db");

    let err = lookup_with_paths(25.25, 87.04, &country_db, Some(&missing_state_db))
        .expect_err("lookup should fail when state DB is missing for India point");

    match err {
        GeoEngineError::StateDatabaseUnavailable { path, .. } => {
            assert_eq!(path, missing_state_db);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn lookup_with_district_path_returns_district_db_error_when_missing() {
    let (country_db, state_db, _) = db_paths();
    let missing_district_db =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("missing_district_in.db");

    let err = lookup_with_district_path(
        25.25,
        87.04,
        &country_db,
        Some(&state_db),
        Some(&missing_district_db),
    )
    .expect_err("lookup should fail when district db is missing for India point");

    match err {
        GeoEngineError::DistrictDatabaseUnavailable { path, .. } => {
            assert_eq!(path, missing_district_db);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn lookup_with_paths_returns_country_not_found_for_invalid_point() {
    let (country_db, state_db, _) = db_paths();

    let err = lookup_with_paths(999.0, 999.0, &country_db, Some(&state_db))
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
fn lookup_with_paths_allows_missing_state_db_for_non_india_points() {
    let (country_db, _, _) = db_paths();

    let result = lookup_with_paths(51.5074, -0.1278, &country_db, None)
        .expect("lookup should succeed for non-India point without state db");

    assert_eq!(result.country.name, "United Kingdom");
    assert_eq!(result.country.iso2, "GB");
    assert!(result.state.is_none());
    assert!(result.district.is_none());
}

#[test]
fn lookup_with_paths_requires_state_db_for_india_points() {
    let (country_db, _, _) = db_paths();

    let err = lookup_with_paths(25.25, 87.04, &country_db, None)
        .expect_err("lookup should fail for India point when state db is omitted");

    match err {
        GeoEngineError::StateDatabaseUnavailable { path, .. } => {
            assert_eq!(path, PathBuf::from("<not provided>"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn lookup_place_formats_district_state_country() {
    let place = lookup_place(25.5941, 85.1376).expect("bundled lookup should format place string");
    assert_eq!(place, "Patna, Bihar, India");
}
