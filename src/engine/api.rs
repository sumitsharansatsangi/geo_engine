use std::path::{Path, PathBuf};

use rkyv::{Archived, string::ArchivedString};

use crate::engine::error::GeoEngineError;
use crate::engine::model::Country;
use crate::engine::{index::SpatialIndex, lookup::find_country, runtime::GeoEngine};

const COUNTRY_DB_PATH: &str = "geo.db";
const STATE_DB_PATH: &str = "state_in.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub name: String,
    pub iso2: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupResult {
    pub country: Region,
    pub state: Option<Region>,
}

pub fn lookup(lat: f32, lon: f32) -> Result<LookupResult, GeoEngineError> {
    lookup_with_paths(lat, lon, Path::new(COUNTRY_DB_PATH), Path::new(STATE_DB_PATH))
}

pub fn lookup_with_paths(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    state_db_path: &Path,
) -> Result<LookupResult, GeoEngineError> {
    let engine = GeoEngine::open(country_db_path)?;
    let index = SpatialIndex::build(engine.countries());
    let country = find_country(lat, lon, &index)?;
    let country_region = region_from_archived(&country.name, &country.iso2);

    if !is_india(country) {
        return Ok(LookupResult {
            country: country_region,
            state: None,
        });
    }

    let state_engine = GeoEngine::open(state_db_path).map_err(|err| match err {
        GeoEngineError::DatabaseOpen { source, .. } | GeoEngineError::DatabaseMap { source, .. } => {
            GeoEngineError::StateDatabaseUnavailable {
                path: PathBuf::from(state_db_path),
                source,
            }
        }
        other => other,
    })?;

    let state_index = SpatialIndex::build(state_engine.countries());
    let state = find_country(lat, lon, &state_index).map_err(|err| match err {
        GeoEngineError::CountryNotFound { lat, lon } => GeoEngineError::StateNotFound { lat, lon },
        other => other,
    })?;

    Ok(LookupResult {
        country: country_region,
        state: Some(region_from_archived(&state.name, &state.iso2)),
    })
}

fn region_from_archived(name: &ArchivedString, iso2: &Archived<[u8; 2]>) -> Region {
    Region {
        name: name.to_string(),
        iso2: String::from_utf8_lossy(&[iso2[0], iso2[1]]).into_owned(),
    }
}

fn is_india(country: &Archived<Country>) -> bool {
    (country.iso2[0] == b'I' && country.iso2[1] == b'N')
        || country.name.as_str().eq_ignore_ascii_case("india")
}
