use std::path::PathBuf;

use geo_engine::{GeoEngineError, lookup_with_subdistrict_path};

fn db_paths() -> (PathBuf, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    (root.join("geo.db"), root.join("subdistrict.db"))
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
}
