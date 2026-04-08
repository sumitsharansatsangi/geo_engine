use std::path::{Path, PathBuf};

use rkyv::{string::ArchivedString, Archived};

use crate::engine::error::GeoEngineError;
use crate::engine::model::Country;
use crate::engine::{index::SpatialIndex, lookup::find_country, runtime::GeoEngine};

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
    pub subdistrict: Option<Region>,
}

pub fn lookup_with_subdistrict_path(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    subdistrict_db_path: Option<&Path>,
) -> Result<LookupResult, GeoEngineError> {
    let engine = GeoEngine::open(country_db_path)?;
    let country = lookup_country(lat, lon, &engine)?;

    if !country.is_india {
        return Ok(LookupResult {
            country: country.region,
            state: None,
            district: None,
            subdistrict: None,
        });
    }

    let resolved_subdistrict_path = resolve_subdistrict_path(country_db_path, subdistrict_db_path);
    let subdistrict_engine = open_subdistrict_engine(&resolved_subdistrict_path)?;

    lookup_india_with_subdistrict_engine(lat, lon, country.region, &subdistrict_engine)
}

fn lookup_india_with_subdistrict_engine(
    lat: f32,
    lon: f32,
    country: Region,
    subdistrict_engine: &GeoEngine,
) -> Result<LookupResult, GeoEngineError> {
    let subdistrict_index = SpatialIndex::build(subdistrict_engine.countries());
    let subdistrict_match = match find_country(lat, lon, &subdistrict_index) {
        Ok(feature) => Some(region_from_archived(&feature.name, &feature.iso2)),
        Err(GeoEngineError::CountryNotFound { .. }) => None,
        Err(other) => return Err(other),
    };

    if let Some(metadata) = subdistrict_match
        .as_ref()
        .and_then(|region| parse_subdistrict_payload(&region.name))
    {
        return Ok(LookupResult {
            country,
            state: Some(Region {
                name: metadata.state_name,
                iso2: metadata.state_code,
            }),
            district: Some(Region {
                name: metadata.district_name,
                iso2: metadata.district_code,
            }),
            subdistrict: Some(Region {
                name: metadata.subdistrict_name,
                iso2: metadata.subdistrict_code,
            }),
        });
    }

    Ok(LookupResult {
        country,
        state: None,
        district: None,
        subdistrict: subdistrict_match,
    })
}

fn open_subdistrict_engine(subdistrict_db_path: &Path) -> Result<GeoEngine, GeoEngineError> {
    GeoEngine::open(subdistrict_db_path).map_err(|err| match err {
        GeoEngineError::DatabaseOpen { source, .. }
        | GeoEngineError::DatabaseMap { source, .. } => {
            GeoEngineError::DistrictDatabaseUnavailable {
                path: PathBuf::from(subdistrict_db_path),
                source,
            }
        }
        other => other,
    })
}

fn resolve_subdistrict_path(country_db_path: &Path, subdistrict_db_path: Option<&Path>) -> PathBuf {
    match subdistrict_db_path {
        Some(path) => path.to_path_buf(),
        None => country_db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("subdistrict.db"),
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

struct SubdistrictMetadata {
    subdistrict_name: String,
    district_name: String,
    state_name: String,
    subdistrict_code: String,
    district_code: String,
    state_code: String,
}

fn parse_subdistrict_payload(payload: &str) -> Option<SubdistrictMetadata> {
    let parts: Vec<&str> = payload.split("||").collect();
    if parts.len() != 6 {
        return None;
    }

    Some(SubdistrictMetadata {
        subdistrict_name: normalize_name(parts[0].trim()),
        district_name: normalize_name(parts[1].trim()),
        state_name: normalize_name(parts[2].trim()),
        subdistrict_code: parts[3].trim().to_string(),
        district_code: parts[4].trim().to_string(),
        state_code: parts[5].trim().to_string(),
    })
}

fn normalize_name(name: &str) -> String {
    let is_all_caps =
        name.chars().any(|c| c.is_alphabetic()) && !name.chars().any(|c| c.is_lowercase());
    if !is_all_caps {
        return name.to_string();
    }

    name.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let rest = chars.as_str().to_lowercase();
                    format!("{}{}", first.to_uppercase(), rest)
                }
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}
