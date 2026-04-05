use std::path::PathBuf;
use std::sync::Once;

use geo_engine::{
    GeoEngineError, init_databases, lookup, lookup_place, lookup_with_district_path,
    lookup_with_paths, lookup_with_subdistrict_path,
};

static INIT: Once = Once::new();

fn db_paths() -> (PathBuf, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    (root.join("geo.db"), root.join("subdistrict.db"))
}

fn ensure_lookup_initialized() {
    INIT.call_once(|| {
        let (country_db, subdistrict_db) = db_paths();
        init_databases(&country_db, &subdistrict_db)
            .expect("global lookup init should succeed for integration tests");
    });
}

#[test]
fn lookup_with_paths_returns_india_admin_hierarchy() {
    let (country_db, _) = db_paths();

    let result = lookup_with_paths(25.25, 87.04, &country_db, None)
        .expect("lookup should succeed for known India/Bihar point");

    assert_eq!(result.country.name, "India");
    assert_eq!(result.country.iso2, "IN");
    assert_eq!(
        result.state.expect("state should be present for India").name,
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
}

#[test]
fn lookup_with_district_path_uses_subdistrict_db_for_backcompat() {
    let (country_db, subdistrict_db) = db_paths();

    let result = lookup_with_district_path(25.25, 87.04, &country_db, None, Some(&subdistrict_db))
        .expect("lookup should succeed when subdistrict db is provided via backcompat API");

    assert_eq!(
        result
            .subdistrict
            .expect("subdistrict should be present for India")
            .name,
        "Sabour"
    );
}

#[test]
fn lookup_with_subdistrict_path_returns_error_when_missing() {
    let (country_db, _) = db_paths();
    let missing_subdistrict_db =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("missing_subdistrict.db");

    let err = lookup_with_subdistrict_path(25.25, 87.04, &country_db, Some(&missing_subdistrict_db))
        .expect_err("lookup should fail when subdistrict db is missing for India point");

    match err {
        GeoEngineError::DistrictDatabaseUnavailable { path, .. } => {
            assert_eq!(path, missing_subdistrict_db);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn lookup_uses_bundled_databases() {
    ensure_lookup_initialized();
    let result = lookup(25.25, 87.04).expect("bundled lookup should succeed");
    assert_eq!(result.country.name, "India");
    assert_eq!(
        result.state.expect("state should be present for India point").name,
        "Bihar"
    );
    assert_eq!(
        result
            .district
            .expect("district should be present for India point")
            .name,
        "Bhagalpur"
    );
    assert_eq!(
        result
            .subdistrict
            .expect("subdistrict should be present for India point")
            .name,
        "Sabour"
    );
}

#[test]
fn bundled_lookup_matches_explicit_subdistrict_lookup() {
    ensure_lookup_initialized();
    let (country_db, subdistrict_db) = db_paths();

    let bundled =
        lookup(25.25, 87.04).expect("bundled lookup should succeed for known India point");
    let explicit = lookup_with_subdistrict_path(25.25, 87.04, &country_db, Some(&subdistrict_db))
        .expect("explicit subdistrict lookup should succeed");

    assert_eq!(bundled.country, explicit.country);
    assert_eq!(bundled.state, explicit.state);
    assert_eq!(bundled.district, explicit.district);
    assert_eq!(bundled.subdistrict, explicit.subdistrict);
}

#[test]
fn lookup_with_paths_returns_country_not_found_for_invalid_point() {
    let (country_db, _) = db_paths();

    let err = lookup_with_paths(999.0, 999.0, &country_db, None)
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
fn lookup_with_paths_allows_non_india_without_subdistrict_db() {
    let (country_db, _) = db_paths();

    let result = lookup_with_paths(51.5074, -0.1278, &country_db, None)
        .expect("lookup should succeed for non-India point");

    assert_eq!(result.country.name, "United Kingdom");
    assert_eq!(result.country.iso2, "GB");
    assert!(result.state.is_none());
    assert!(result.district.is_none());
    assert!(result.subdistrict.is_none());
}

#[test]
fn lookup_place_formats_subdistrict_district_state_country() {
    ensure_lookup_initialized();
    let place = lookup_place(25.5941, 85.1376).expect("bundled lookup should format place string");
    assert_eq!(place, "Patna Rural, Patna, Bihar, India");
}
