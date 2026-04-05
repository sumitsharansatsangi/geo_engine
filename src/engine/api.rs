use std::io;
use std::path::{Path, PathBuf};

use rkyv::{Archived, string::ArchivedString};

use crate::engine::error::GeoEngineError;
use crate::engine::model::Country;
use crate::engine::{index::SpatialIndex, lookup::find_country, runtime::GeoEngine};

macro_rules! include_bytes_aligned {
    ($align_ty:ty, $path:literal) => {{
        #[repr(C)]
        struct AlignedAs<Align, Bytes: ?Sized> {
            _align: [Align; 0],
            bytes: Bytes,
        }

        static ALIGNED: &AlignedAs<$align_ty, [u8]> = &AlignedAs {
            _align: [],
            bytes: *include_bytes!($path),
        };

        &ALIGNED.bytes
    }};
}

static BUNDLED_COUNTRY_DB: &[u8] = include_bytes_aligned!(u32, "../../geo.db");
static BUNDLED_STATE_DB: &[u8] = include_bytes_aligned!(u32, "../../state_in.db");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub name: String,
    pub iso2: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupResult {
    pub country: Region,
    pub state: Option<Region>,
    pub district: Option<Region>,
}

pub fn lookup_place(lat: f32, lon: f32) -> Result<String, GeoEngineError> {
    let result = lookup(lat, lon)?;
    Ok(format_place(&result))
}

pub fn lookup(lat: f32, lon: f32) -> Result<LookupResult, GeoEngineError> {
    let country_engine = GeoEngine::from_static_bytes(BUNDLED_COUNTRY_DB);
    let country = lookup_country(lat, lon, &country_engine)?;

    if !country.is_india {
        return Ok(LookupResult {
            country: country.region,
            state: None,
            district: None,
        });
    }

    let district_db_path = Path::new("district_in.db");
    if !district_db_path.exists() {
        panic!(
            "❌ district_in.db missing\n\
            👉 Run: cargo run --bin build_district_db\n\
            👉 Expected location: {:?}",
            district_db_path
        );
    }

    let district_engine = GeoEngine::open(district_db_path).map_err(|err| match err {
        GeoEngineError::DatabaseOpen { source, .. }
        | GeoEngineError::DatabaseMap { source, .. } => GeoEngineError::DistrictDatabaseUnavailable {
            path: PathBuf::from(district_db_path),
            source,
        },
        other => other,
    })?;

    lookup_india_with_engines(
        lat,
        lon,
        country.region,
        Some(GeoEngine::from_static_bytes(BUNDLED_STATE_DB)),
        Some(district_engine),
    )
}

pub fn lookup_with_paths(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    state_db_path: Option<&Path>,
) -> Result<LookupResult, GeoEngineError> {
    lookup_with_district_path(lat, lon, country_db_path, state_db_path, None)
}

pub fn lookup_with_district_path(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    state_db_path: Option<&Path>,
    district_db_path: Option<&Path>,
) -> Result<LookupResult, GeoEngineError> {
    let engine = GeoEngine::open(country_db_path)?;
    let country = lookup_country(lat, lon, &engine)?;

    if !country.is_india {
        return Ok(LookupResult {
            country: country.region,
            state: None,
            district: None,
        });
    }

    let state_engine = open_state_engine(state_db_path.ok_or_else(missing_state_database_error)?)?;
    let district_engine =
        district_db_path
            .map(GeoEngine::open)
            .transpose()
            .map_err(|err| match err {
                GeoEngineError::DatabaseOpen { source, .. }
                | GeoEngineError::DatabaseMap { source, .. } => {
                    GeoEngineError::DistrictDatabaseUnavailable {
                        path: PathBuf::from(
                            district_db_path.expect("district path should be available"),
                        ),
                        source,
                    }
                }
                other => other,
            })?;

    lookup_india_with_engines(lat, lon, country.region, Some(state_engine), district_engine)
}

fn lookup_india_with_engines(
    lat: f32,
    lon: f32,
    country: Region,
    state_engine: Option<GeoEngine>,
    district_engine: Option<GeoEngine>,
) -> Result<LookupResult, GeoEngineError> {
    let state_engine = state_engine.ok_or_else(|| GeoEngineError::StateDatabaseUnavailable {
        path: PathBuf::from("<not provided>"),
        source: io::Error::new(
            io::ErrorKind::NotFound,
            "state lookup required but state database was not provided",
        ),
    })?;

    let state_index = SpatialIndex::build(state_engine.countries());
    let state = find_country(lat, lon, &state_index).map_err(|err| match err {
        GeoEngineError::CountryNotFound { lat, lon } => GeoEngineError::StateNotFound { lat, lon },
        other => other,
    })?;

    let district = district_engine
        .map(|engine| {
            let district_index = SpatialIndex::build(engine.countries());
            match find_country(lat, lon, &district_index) {
                Ok(district) => Ok(Some(region_from_archived(&district.name, &district.iso2))),
                Err(GeoEngineError::CountryNotFound { .. }) => Ok(None),
                Err(other) => Err(other),
            }
        })
        .transpose()?;

    Ok(LookupResult {
        country,
        state: Some(region_from_archived(&state.name, &state.iso2)),
        district: district.flatten(),
    })
}

fn open_state_engine(state_db_path: &Path) -> Result<GeoEngine, GeoEngineError> {
    GeoEngine::open(state_db_path).map_err(|err| match err {
        GeoEngineError::DatabaseOpen { source, .. }
        | GeoEngineError::DatabaseMap { source, .. } => GeoEngineError::StateDatabaseUnavailable {
            path: PathBuf::from(state_db_path),
            source,
        },
        other => other,
    })
}

fn missing_state_database_error() -> GeoEngineError {
    GeoEngineError::StateDatabaseUnavailable {
        path: PathBuf::from("<not provided>"),
        source: io::Error::new(
            io::ErrorKind::NotFound,
            "state lookup required but state database was not provided",
        ),
    }
}

struct CountryLookup {
    region: Region,
    is_india: bool,
}

fn lookup_country(lat: f32, lon: f32, engine: &GeoEngine) -> Result<CountryLookup, GeoEngineError> {
    let index = SpatialIndex::build(engine.countries());
    let country = find_country(lat, lon, &index)?;
    Ok(CountryLookup {
        region: region_from_archived(&country.name, &country.iso2),
        is_india: is_india(country),
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

fn format_place(result: &LookupResult) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(3);
    if let Some(district) = result.district.as_ref() {
        parts.push(district.name.as_str());
    }
    if let Some(state) = result.state.as_ref() {
        parts.push(state.name.as_str());
    }
    parts.push(result.country.name.as_str());
    parts.join(", ")
}
