use std::path::PathBuf;

use geo_engine::{GeoEngineError, lookup, lookup_with_paths};

fn db_paths() -> (PathBuf, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    (root.join("geo.db"), root.join("state_in.db"))
}

#[test]
fn lookup_with_paths_returns_country_and_state() {
    let (country_db, state_db) = db_paths();

    let result = lookup_with_paths(25.25, 87.04, &country_db, &state_db)
        .expect("lookup should succeed for known India/Bihar point");

    assert_eq!(result.country.name, "India");
    assert_eq!(result.country.iso2, "IN");

    let state = result.state.expect("state should be present for India point");
    assert_eq!(state.name, "Bihar");
}

#[test]
fn lookup_uses_bundled_databases() {
    let result = lookup(25.25, 87.04).expect("bundled lookup should succeed");
    assert_eq!(result.country.name, "India");

    let state = result.state.expect("state should be present for India point");
    assert_eq!(state.name, "Bihar");
}

#[test]
fn lookup_with_paths_returns_state_db_error_when_missing() {
    let (country_db, _) = db_paths();
    let missing_state_db = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("missing_state_in.db");

    let err = lookup_with_paths(25.25, 87.04, &country_db, &missing_state_db)
        .expect_err("lookup should fail when state DB is missing for India point");

    match err {
        GeoEngineError::StateDatabaseUnavailable { path, .. } => {
            assert_eq!(path, missing_state_db);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn lookup_with_paths_returns_country_not_found_for_invalid_point() {
    let (country_db, state_db) = db_paths();

    let err = lookup_with_paths(999.0, 999.0, &country_db, &state_db)
        .expect_err("lookup should fail for impossible coordinates");

    match err {
        GeoEngineError::CountryNotFound { lat, lon } => {
            assert_eq!(lat, 999.0);
            assert_eq!(lon, 999.0);
        }
        other => panic!("unexpected error: {other}"),
    }
}
