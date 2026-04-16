use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
};

use fst::{IntoStreamer, Map, Streamer};
use rayon::prelude::*;
use rkyv::{Archived, string::ArchivedString};
use serde::Serialize;

use crate::engine::city::{City, CityCore, CityMeta, normalize};
use crate::engine::error::GeoEngineError;
use crate::engine::h3::{
    H3RuntimeIndex, default_sidecar_path as default_h3_sidecar_path,
    merge_candidate_ids as merge_h3_candidate_ids,
};
use crate::engine::model::Country;
use crate::engine::subdistrict_meta::SubdistrictMeta;
use crate::engine::{
    bootstrap::init_all_assets,
    index::SpatialIndex,
    lookup::{find_country, prefilter_bbox_candidates},
    runtime::GeoEngine,
    spatial::{SpatialRuntimeIndex, default_sidecar_path, merge_candidate_ids},
};

// Separator constants for parsing subdistrict metadata
const SUBDISTRICT_FIELD_SEPARATOR: &str = "||";
const LANGUAGE_ENTRY_SEPARATOR: &str = "##";
const LANGUAGE_COMPONENT_SEPARATOR: &str = "~~";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeoLanguage {
    pub code: String,
    pub name: String,
    pub usage_type: String,
}

/// A geographic region with area code (country/state/district/subdistrict).
///
/// Fields:
/// - `name`: Display name of the region (e.g., "India", "Bihar", "Sabour")
/// - `iso2`: Two-character ISO code or internal identifier
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Region {
    pub name: String,
    pub iso2: String,
}

/// Result of a geographic coordinate lookup.
///
/// Contains the country and optional Indian administrative hierarchy
/// for the given coordinates, along with demographics and location of
/// the polygon center.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LookupResult {
    pub country: Region,
    pub state: Option<Region>,
    pub district: Option<Region>,
    pub subdistrict: Option<Region>,
    pub demographics: Option<DistrictDemographics>,
    pub latitude: f32,
    pub longitude: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DistrictDemographics {
    pub district_uni_code: String,
    pub major_religion: String,
    pub languages: Vec<GeoLanguage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubdistrictMatch {
    pub subdistrict: Region,
    pub district: Region,
    pub state: Region,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CityMatch {
    pub geoname_id: u32,
    pub name: String,
    pub ascii: String,
    pub country_name: String,
    pub country_code: String,
    pub admin1_name: Option<String>,
    pub admin1_code: Option<String>,
    pub admin2_name: Option<String>,
    pub admin2_code: Option<String>,
    pub latitude: f32,
    pub longitude: f32,
}

/// Combined search results from both city and subdistrict databases.
///
/// Used by the `search()` function to return matches from both
/// city (geonames) and Indian administrative division (subdistrict) sources.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CombinedSearchResult {
    pub cities: Vec<CityMatch>,
    pub subdistricts: Vec<SubdistrictMatch>,
}

/// Result of reverse geocoding (coordinates to location).
///
/// Includes administrative hierarchy up to the subdistrict level
/// and the nearest city match.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReverseGeocodingResult {
    pub country: Region,
    pub state: Option<Region>,
    pub district: Option<Region>,
    pub subdistrict: Option<Region>,
    pub city: CityMatch,
}

pub struct InitializedGeoEngine {
    engine: EngineBundle,
}

struct EngineBundle {
    country_shards: Vec<CountryShard>,
    country_names: HashMap<String, String>,
    subdistrict: Option<GeoEngine>,
    subdistrict_index: Option<SpatialIndex>,
    subdistrict_db_path: PathBuf,
    subdistrict_meta: Option<Vec<SubdistrictMetadata>>,
    city_index: Option<CityIndex>,
}

struct CountryShard {
    engine: GeoEngine,
    index: SpatialIndex,
    spatial_index: Option<SpatialRuntimeIndex>,
    h3_index: Option<H3RuntimeIndex>,
}

struct CityIndex {
    fst: Option<Map<Vec<u8>>>,
    cities_by_id: HashMap<u32, City>,
    city_core_path: PathBuf,
}

struct InitializedPaths {
    asset_dir: PathBuf,
    country_db_path: PathBuf,
    subdistrict_db_path: PathBuf,
    subdistrict_meta_path: PathBuf,
    city_fst_path: PathBuf,
    city_core_path: PathBuf,
    city_meta_path: PathBuf,
}

static PATHS: OnceLock<InitializedPaths> = OnceLock::new();
static ENGINE: OnceLock<Result<InitializedGeoEngine, String>> = OnceLock::new();

/// Initialize the global geo engine with database paths.
///
/// This must be called once before using `search()` or `reverse_geocoding()`.
/// Paths are cached after first initialization. Subsequent calls with identical
/// paths are allowed; different paths will return `PathsAlreadyInitialized` error.
///
/// # Arguments
/// * `asset_dir` - Directory where geo assets are located or will be downloaded
///
/// # Returns
/// * `Ok(true)` when initialization succeeds
pub fn init_path(asset_dir: String, verify_checksum: bool) -> Result<bool, GeoEngineError> {
    let asset_dir = normalize_asset_dir(Path::new(&asset_dir));

    if let Some(initialized_paths) = PATHS.get() {
        if initialized_paths.asset_dir != asset_dir {
            return Err(GeoEngineError::PathsAlreadyInitialized);
        }

        if let Err(err) = get_initialized_engine() {
            return Err(engine_initialization_failed(&asset_dir, err));
        }

        return Ok(true);
    }

    let asset_paths = match init_all_assets(&asset_dir, verify_checksum) {
        Ok(paths) => paths,
        Err(err) => {
            return Err(engine_initialization_failed(&asset_dir, err));
        }
    };

    let candidate_paths = InitializedPaths {
        asset_dir: asset_dir.clone(),
        country_db_path: asset_paths.geo_db_path,
        subdistrict_db_path: asset_paths.subdistrict_db_path,
        subdistrict_meta_path: asset_paths.subdistrict_meta_path,
        city_fst_path: asset_paths.city_fst_path,
        city_core_path: asset_paths.city_core_path,
        city_meta_path: asset_paths.city_meta_path,
    };
 
    let initialized_paths = PATHS.get_or_init(|| candidate_paths);

    let same_paths = initialized_paths.asset_dir == asset_dir;

    if !same_paths {
        return Err(GeoEngineError::PathsAlreadyInitialized);
    }

    if let Err(err) = get_initialized_engine() {
        return Err(engine_initialization_failed(&asset_dir, err));
    }

    Ok(true)
}

fn engine_initialization_failed(asset_dir: &Path, err: impl std::fmt::Display) -> GeoEngineError {
    GeoEngineError::EngineInitializationFailed {
        message: format!(
            "failed to initialize engine for '{}': {}",
            asset_dir.display(),
            err
        ),
    }
}

fn normalize_asset_dir(asset_dir: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(asset_dir) {
        return canonical;
    }

    if asset_dir.is_absolute() {
        return asset_dir.to_path_buf();
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(asset_dir))
        .unwrap_or_else(|_| asset_dir.to_path_buf())
}

fn get_paths() -> Result<&'static InitializedPaths, GeoEngineError> {
    PATHS.get().ok_or(GeoEngineError::PathsNotInitialized)
}

fn get_initialized_engine() -> Result<&'static InitializedGeoEngine, GeoEngineError> {
    let paths = get_paths()?;
    let result = ENGINE.get_or_init(|| {
        InitializedGeoEngine::open(
            &paths.country_db_path,
            paths.subdistrict_db_path.as_path(),
            paths.subdistrict_meta_path.as_path(),
            paths.city_fst_path.as_path(),
            paths.city_core_path.as_path(),
            paths.city_meta_path.as_path(),
        )
        .map_err(|err| err.to_string())
    });

    match result {
        Ok(engine) => Ok(engine),
        Err(message) => Err(GeoEngineError::EngineInitializationFailed {
            message: message.clone(),
        }),
    }
}

/// Reverse geocode coordinates to location (lat/lon → location name).
///
/// Requires prior initialization via `init_path()`.
/// Returns the country, optional administrative hierarchy (if India),
/// and the nearest city.
///
/// # Arguments
/// * `lat` - Latitude (-90 to 90)
/// * `lon` - Longitude (-180 to 180)
pub fn reverse_geocoding(lat: f32, lon: f32) -> Result<ReverseGeocodingResult, GeoEngineError> {
    let engine = get_initialized_engine().map_err(|err| {
        crate::operation_failed!("api", "reverse_geocoding", "load_initialized_engine", err)
    })?;
    engine
        .reverse_geocoding(lat, lon)
        .map_err(|err| crate::operation_failed!("api", "reverse_geocoding", "execute", err))
}

pub fn reverse_geocoding_batch(
    coordinates: &[(f32, f32)],
) -> Result<Vec<Result<ReverseGeocodingResult, GeoEngineError>>, GeoEngineError> {
    let engine = get_initialized_engine().map_err(|err| {
        crate::operation_failed!(
            "api",
            "reverse_geocoding_batch",
            "load_initialized_engine",
            err
        )
    })?;
    Ok(coordinates
        .par_iter()
        .map(|(lat, lon)| engine.reverse_geocoding(*lat, *lon))
        .collect())
}

/// Search for cities or subdistricts by name.
///
/// Requires prior initialization via `init_path()`.
/// Returns combined results from both city (geonames) and
/// Indian administrative division (subdistrict) sources.
///
/// # Arguments
/// * `query` - Search term (case-insensitive, supports prefix matching)
pub fn search(query: &str) -> Result<CombinedSearchResult, GeoEngineError> {
    let engine = get_initialized_engine().map_err(|err| {
        crate::operation_failed!("api", "search", "load_initialized_engine", err)
    })?;
    engine
        .search_places_by_name(query, None)
        .map_err(|err| crate::operation_failed!("api", "search", "execute", err))
}

pub fn search_batch(
    queries: &[String],
) -> Result<Vec<Result<CombinedSearchResult, GeoEngineError>>, GeoEngineError> {
    let engine = get_initialized_engine()
        .map_err(|err| crate::operation_failed!("api", "search_batch", "load_initialized_engine", err))?;
    Ok(queries
        .par_iter()
        .map(|query| engine.search_places_by_name(query, None))
        .collect())
}

impl InitializedGeoEngine {
    pub fn open(
        country_db_path: &Path,
        subdistrict_db_path: &Path,
        subdistrict_meta_path: &Path,
        city_fst_path: &Path,
        city_core_path: &Path,
        city_meta_path: &Path,
    ) -> Result<Self, GeoEngineError> {
        let country_shards = load_country_shards(country_db_path)?;
        let country_names = load_country_names(&country_shards);

        let subdistrict_engine = Some(open_subdistrict_engine(subdistrict_db_path)?);
        let subdistrict_index = subdistrict_engine
            .as_ref()
            .map(|engine| SpatialIndex::build(engine.countries()));
        let subdistrict_meta = Some(load_subdistrict_meta(subdistrict_meta_path)?);
        let city_index = load_city_index(
            Some(city_fst_path),
            Some(city_core_path),
            Some(city_meta_path),
        )?;

        Ok(Self {
            engine: EngineBundle {
                country_shards,
                country_names,
                subdistrict: subdistrict_engine,
                subdistrict_index,
                subdistrict_db_path: subdistrict_db_path.to_path_buf(),
                subdistrict_meta,
                city_index,
            },
        })
    }

    #[cfg_attr(not(all(feature = "wasm", target_arch = "wasm32")), allow(dead_code))]
    pub fn open_from_bytes(
        country_db_bytes: &[u8],
        subdistrict_db_bytes: Option<&[u8]>,
        subdistrict_meta_bytes: Option<&[u8]>,
        city_fst_bytes: Option<&[u8]>,
        city_core_bytes: Option<&[u8]>,
        city_meta_bytes: Option<&[u8]>,
    ) -> Result<Self, GeoEngineError> {
        let country_engine = GeoEngine::from_bytes(country_db_bytes, "country.db")?;
        let country_shards = vec![CountryShard {
            index: SpatialIndex::build(country_engine.countries()),
            engine: country_engine,
            spatial_index: None,
            h3_index: None,
        }];
        let country_names = load_country_names(&country_shards);

        let subdistrict_engine = if let Some(bytes) = subdistrict_db_bytes {
            Some(GeoEngine::from_bytes(bytes, "subdistrict.db")?)
        } else {
            None
        };

        let subdistrict_index = subdistrict_engine
            .as_ref()
            .map(|engine| SpatialIndex::build(engine.countries()));
        let subdistrict_meta = if let Some(bytes) = subdistrict_meta_bytes {
            Some(load_subdistrict_meta_from_bytes(bytes)?)
        } else {
            None
        };

        let city_index =
            load_city_index_from_bytes(city_fst_bytes, city_core_bytes, city_meta_bytes)?;

        Ok(Self {
            engine: EngineBundle {
                country_shards,
                country_names,
                subdistrict: subdistrict_engine,
                subdistrict_index,
                subdistrict_db_path: PathBuf::from("subdistrict.db"),
                subdistrict_meta,
                city_index,
            },
        })
    }

    pub fn lookup(&self, lat: f32, lon: f32) -> Result<LookupResult, GeoEngineError> {
        let country = lookup_country_from_shards(lat, lon, &self.engine.country_shards)?;

        if !country.is_india {
            return Ok(LookupResult {
                country: country.region,
                state: None,
                district: None,
                subdistrict: None,
                demographics: None,
                latitude: lat,
                longitude: lon,
            });
        }

        let Some(subdistrict_engine) = self.engine.subdistrict.as_ref() else {
            return Err(GeoEngineError::DistrictDatabaseUnavailable {
                path: self.engine.subdistrict_db_path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "subdistrict database not initialized",
                ),
            });
        };

        let subdistrict_index = self.engine.subdistrict_index.as_ref().ok_or_else(|| {
            GeoEngineError::DistrictDatabaseUnavailable {
                path: self.engine.subdistrict_db_path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "subdistrict spatial index not initialized",
                ),
            }
        })?;

        let subdistrict_match_with_center =
            match find_country(lat, lon, subdistrict_index, subdistrict_engine.countries()) {
                Ok(feature) => {
                    let region = region_from_archived(&feature.name, &feature.iso2);
                    let center = calculate_polygon_center(&feature.polygons);
                    Some((region, center))
                }
                Err(GeoEngineError::CountryNotFound { .. }) => None,
                Err(other) => return Err(other),
            };

        if let Some((subdistrict_match, (center_lat, center_lon))) =
            subdistrict_match_with_center.as_ref()
            && let Some(metadata) = resolve_subdistrict_metadata(
                &subdistrict_match.name,
                self.engine.subdistrict_meta.as_deref(),
            )
        {
            return Ok(LookupResult {
                country: country.region,
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
                demographics: metadata.demographics,
                latitude: *center_lat,
                longitude: *center_lon,
            });
        }

        let (fallback_lat, fallback_lon) = subdistrict_match_with_center
            .as_ref()
            .map(|(_, center)| *center)
            .unwrap_or((lat, lon));

        Ok(LookupResult {
            country: country.region,
            state: None,
            district: None,
            subdistrict: subdistrict_match_with_center.map(|(region, _)| region),
            demographics: None,
            latitude: fallback_lat,
            longitude: fallback_lon,
        })
    }

    pub fn reverse_geocoding(
        &self,
        lat: f32,
        lon: f32,
    ) -> Result<ReverseGeocodingResult, GeoEngineError> {
        let lookup = self.lookup(lat, lon)?;
        let city = self.nearest_city(lat, lon)?;

        if lookup.state.is_none() && lookup.district.is_none() {
            return Ok(ReverseGeocodingResult {
                country: lookup.country,
                state: None,
                district: None,
                subdistrict: None,
                city,
            });
        }

        Ok(ReverseGeocodingResult {
            country: lookup.country,
            state: lookup.state,
            district: lookup.district,
            subdistrict: lookup.subdistrict,
            city,
        })
    }

    pub fn search_places_by_name(
        &self,
        query: &str,
        city_limit: Option<usize>,
    ) -> Result<CombinedSearchResult, GeoEngineError> {
        let subdistricts = self.search_subdistricts_by_name(query)?;
        let cities = self.search_cities_by_name(query, city_limit)?;

        Ok(CombinedSearchResult {
            cities,
            subdistricts,
        })
    }

    fn search_subdistricts_by_name(
        &self,
        query: &str,
    ) -> Result<Vec<SubdistrictMatch>, GeoEngineError> {
        let normalized_query = query.trim();
        if normalized_query.is_empty() {
            return Ok(Vec::new());
        }

        let Some(engine) = self.engine.subdistrict.as_ref() else {
            return Err(GeoEngineError::DistrictDatabaseUnavailable {
                path: self.engine.subdistrict_db_path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "subdistrict database not initialized",
                ),
            });
        };

        let query_lower = normalized_query.to_lowercase();
        let mut matches = Vec::new();

        for feature in engine.countries().iter() {
            let Some(metadata) = resolve_subdistrict_metadata(
                feature.name.as_str(),
                self.engine.subdistrict_meta.as_deref(),
            ) else {
                continue;
            };

            if !metadata
                .subdistrict_name
                .to_lowercase()
                .contains(&query_lower)
            {
                continue;
            }

            matches.push(SubdistrictMatch {
                subdistrict: Region {
                    name: metadata.subdistrict_name,
                    iso2: metadata.subdistrict_code,
                },
                district: Region {
                    name: metadata.district_name,
                    iso2: metadata.district_code,
                },
                state: Region {
                    name: metadata.state_name,
                    iso2: metadata.state_code,
                },
            });
        }

        matches.sort_by(|left, right| {
            left.subdistrict
                .name
                .cmp(&right.subdistrict.name)
                .then_with(|| left.district.name.cmp(&right.district.name))
                .then_with(|| left.state.name.cmp(&right.state.name))
        });

        Ok(matches)
    }

    fn search_cities_by_name(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<CityMatch>, GeoEngineError> {
        let normalized = normalize(query.trim());
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let Some(city_index) = self.engine.city_index.as_ref() else {
            return Ok(Vec::new());
        };

        let Some(fst) = city_index.fst.as_ref() else {
            return Ok(Vec::new());
        };

        let prefix = format!("{}|", normalized);
        let upper = format!("{}\u{10FFFF}", prefix);
        let mut stream = fst
            .range()
            .ge(prefix.as_str())
            .lt(upper.as_str())
            .into_stream();

        let max_results = limit.map(|value| value.max(1));
        let mut matched_ids: BTreeSet<u32> = BTreeSet::new();
        while let Some((_key, value)) = stream.next() {
            matched_ids.insert(value as u32);
            if let Some(max_results) = max_results
                && matched_ids.len() >= max_results
            {
                break;
            }
        }

        let mut matches: Vec<CityMatch> = matched_ids
            .into_iter()
            .filter_map(|geoname_id| {
                city_index
                    .cities_by_id
                    .get(&geoname_id)
                    .map(|city| CityMatch {
                        geoname_id: city.geoname_id,
                        name: city.name.clone(),
                        ascii: city.ascii.clone(),
                        country_name: self
                            .engine
                            .country_names
                            .get(&city.country_code)
                            .cloned()
                            .unwrap_or_else(|| city.country_code.clone()),
                        country_code: city.country_code.clone(),
                        admin1_name: city.admin1_name.clone(),
                        admin1_code: city.admin1_code.clone(),
                        admin2_name: city.admin2_name.clone(),
                        admin2_code: city.admin2_code.clone(),
                        latitude: city.lat,
                        longitude: city.lon,
                    })
            })
            .collect();

        matches.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.country_code.cmp(&right.country_code))
                .then_with(|| left.geoname_id.cmp(&right.geoname_id))
        });

        Ok(matches)
    }

    fn nearest_city(&self, lat: f32, lon: f32) -> Result<CityMatch, GeoEngineError> {
        let Some(city_index) = self.engine.city_index.as_ref() else {
            return Err(GeoEngineError::DatabaseMap {
                path: PathBuf::from("city.core"),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "city dataset not initialized",
                ),
            });
        };

        nearest_city_from_map(
            lat,
            lon,
            &city_index.cities_by_id,
            &city_index.city_core_path,
            &self.engine.country_names,
        )
    }
}

fn nearest_city_from_map(
    lat: f32,
    lon: f32,
    cities_by_id: &HashMap<u32, City>,
    city_core_path: &Path,
    country_names: &HashMap<String, String>,
) -> Result<CityMatch, GeoEngineError> {
    let nearest = cities_by_id
        .values()
        .min_by(|left, right| {
            let left_distance = haversine_km(lat, lon, left.lat, left.lon);
            let right_distance = haversine_km(lat, lon, right.lat, right.lon);
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| GeoEngineError::DatabaseMap {
            path: city_core_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "no cities found in city dataset",
            ),
        })?;

    Ok(CityMatch {
        geoname_id: nearest.geoname_id,
        name: nearest.name.clone(),
        ascii: nearest.ascii.clone(),
        country_name: country_name_for_code(&nearest.country_code, country_names),
        country_code: nearest.country_code.clone(),
        admin1_name: nearest.admin1_name.clone(),
        admin1_code: nearest.admin1_code.clone(),
        admin2_name: nearest.admin2_name.clone(),
        admin2_code: nearest.admin2_code.clone(),
        latitude: nearest.lat,
        longitude: nearest.lon,
    })
}

fn load_city_index(
    city_fst_path: Option<&Path>,
    city_core_path: Option<&Path>,
    city_meta_path: Option<&Path>,
) -> Result<Option<CityIndex>, GeoEngineError> {
    let Some(city_core_path) = city_core_path else {
        return Ok(None);
    };
    let Some(city_meta_path) = city_meta_path else {
        return Ok(None);
    };

    let cities_by_id = load_cities_by_id(city_core_path, city_meta_path)?;
    let fst = if let Some(city_fst_path) = city_fst_path {
        let fst_bytes = fs::read(city_fst_path).map_err(|source| GeoEngineError::DatabaseOpen {
            path: city_fst_path.to_path_buf(),
            source,
        })?;
        Some(
            Map::new(fst_bytes).map_err(|err| GeoEngineError::DatabaseMap {
                path: city_fst_path.to_path_buf(),
                source: std::io::Error::other(err.to_string()),
            })?,
        )
    } else {
        None
    };

    Ok(Some(CityIndex {
        fst,
        cities_by_id,
        city_core_path: city_core_path.to_path_buf(),
    }))
}

#[cfg_attr(not(all(feature = "wasm", target_arch = "wasm32")), allow(dead_code))]
fn load_city_index_from_bytes(
    city_fst_bytes: Option<&[u8]>,
    city_core_bytes: Option<&[u8]>,
    city_meta_bytes: Option<&[u8]>,
) -> Result<Option<CityIndex>, GeoEngineError> {
    let Some(city_core_bytes) = city_core_bytes else {
        return Ok(None);
    };
    let Some(city_meta_bytes) = city_meta_bytes else {
        return Ok(None);
    };

    let cities_by_id = load_cities_by_id_from_bytes(city_core_bytes, city_meta_bytes)?;
    let fst = if let Some(city_fst_bytes) = city_fst_bytes {
        Some(
            Map::new(city_fst_bytes.to_vec()).map_err(|err| GeoEngineError::DatabaseMap {
                path: PathBuf::from("cities.fst"),
                source: std::io::Error::other(err.to_string()),
            })?,
        )
    } else {
        None
    };

    Ok(Some(CityIndex {
        fst,
        cities_by_id,
        city_core_path: PathBuf::from("cities.core"),
    }))
}

fn haversine_km(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let r = 6371.0f32;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let lat1r = lat1.to_radians();
    let lat2r = lat2.to_radians();

    let a = (dlat / 2.0).sin().powi(2) + lat1r.cos() * lat2r.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    r * c
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

fn load_country_shards(country_db_path: &Path) -> Result<Vec<CountryShard>, GeoEngineError> {
    if country_db_path.is_file() {
        let engine = GeoEngine::open(country_db_path)?;
        let index = SpatialIndex::build(engine.countries());
        let spatial_index = load_country_spatial_index(country_db_path);
        let h3_index = load_country_h3_index(country_db_path);
        return Ok(vec![CountryShard {
            engine,
            index,
            spatial_index,
            h3_index,
        }]);
    }

    if !country_db_path.is_dir() {
        return Err(GeoEngineError::DatabaseOpen {
            path: country_db_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "country database path is neither a file nor a directory",
            ),
        });
    }

    let mut entries: Vec<PathBuf> = fs::read_dir(country_db_path)
        .map_err(|source| GeoEngineError::DatabaseOpen {
            path: country_db_path.to_path_buf(),
            source,
        })?
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("db") || ext.eq_ignore_ascii_case("zst"))
                .unwrap_or(false)
        })
        .collect();

    entries.sort();
    if entries.is_empty() {
        return Err(GeoEngineError::DatabaseOpen {
            path: country_db_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no country shard files (*.db or *.zst) found in directory",
            ),
        });
    }

    let mut shards = Vec::with_capacity(entries.len());
    for shard_path in entries {
        let engine = GeoEngine::open(&shard_path)?;
        let index = SpatialIndex::build(engine.countries());
        let spatial_index = load_country_spatial_index(&shard_path);
        let h3_index = load_country_h3_index(&shard_path);
        shards.push(CountryShard {
            engine,
            index,
            spatial_index,
            h3_index,
        });
    }

    Ok(shards)
}

fn load_country_names(shards: &[CountryShard]) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for shard in shards {
        for country in shard.engine.countries().iter() {
            let code = String::from_utf8_lossy(&[country.iso2[0], country.iso2[1]]).into_owned();
            names
                .entry(code)
                .or_insert_with(|| country.name.to_string());
        }
    }

    names
}

fn lookup_country_from_shards(
    lat: f32,
    lon: f32,
    country_shards: &[CountryShard],
) -> Result<CountryLookup, GeoEngineError> {
    for shard in country_shards {
        match lookup_country(
            lat,
            lon,
            shard.engine.countries(),
            &shard.index,
            shard.spatial_index.as_ref(),
            shard.h3_index.as_ref(),
        ) {
            Ok(found) => return Ok(found),
            Err(GeoEngineError::CountryNotFound { .. }) => continue,
            Err(err) => return Err(err),
        }
    }

    Err(GeoEngineError::CountryNotFound { lat, lon })
}

fn country_name_for_code(code: &str, country_names: &HashMap<String, String>) -> String {
    country_names
        .get(code)
        .cloned()
        .unwrap_or_else(|| code.to_string())
}

fn load_country_spatial_index(country_db_path: &Path) -> Option<SpatialRuntimeIndex> {
    if std::env::var("GEO_ENGINE_DISABLE_SPATIAL_INDEX")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return None;
    }

    let sidecar_path = default_sidecar_path(country_db_path);
    match SpatialRuntimeIndex::from_file(&sidecar_path) {
        Ok(index) => Some(index),
        Err(GeoEngineError::DatabaseOpen { .. }) => None,
        Err(err) => {
            eprintln!(
                "geo_engine: failed to load spatial sidecar '{}': {}",
                sidecar_path.display(),
                err
            );
            None
        }
    }
}

fn load_country_h3_index(country_db_path: &Path) -> Option<H3RuntimeIndex> {
    if std::env::var("GEO_ENGINE_DISABLE_SPATIAL_INDEX")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return None;
    }

    let sidecar_path = default_h3_sidecar_path(country_db_path);
    match H3RuntimeIndex::from_file(&sidecar_path) {
        Ok(index) => Some(index),
        Err(GeoEngineError::DatabaseOpen { .. }) => None,
        Err(err) => {
            eprintln!(
                "geo_engine: failed to load h3 sidecar '{}': {}",
                sidecar_path.display(),
                err
            );
            None
        }
    }
}

struct CountryLookup {
    region: Region,
    is_india: bool,
}

fn lookup_country(
    lat: f32,
    lon: f32,
    countries: &Archived<Vec<Country>>,
    index: &SpatialIndex,
    spatial_index: Option<&SpatialRuntimeIndex>,
    h3_index: Option<&H3RuntimeIndex>,
) -> Result<CountryLookup, GeoEngineError> {
    let rtree_candidates = index.candidates(lat, lon);
    let cell_candidates = spatial_index.and_then(|idx| idx.candidate_country_ids(lat, lon));
    let candidates = merge_candidate_ids(cell_candidates, rtree_candidates);
    let h3_candidates = h3_index.and_then(|idx| idx.candidate_ids(lat, lon));
    let candidates = merge_h3_candidate_ids(h3_candidates, candidates.into_iter());

    let candidates = prefilter_bbox_candidates(lat, lon, countries, candidates);
    let allowed_countries: HashSet<u32> = candidates.iter().copied().collect();

    if let Some(spatial_index) = spatial_index {
        let polygon_candidates = spatial_index.polygon_candidates(lat, lon, Some(&candidates));
        for (country_id, ring_id) in polygon_candidates {
            let Some(country) = countries.get(country_id as usize) else {
                continue;
            };
            let Some(ring) = country.polygons.get(ring_id as usize) else {
                continue;
            };

            if crate::engine::polygon::point_in_ring(lat, lon, ring) {
                return Ok(CountryLookup {
                    region: region_from_archived(&country.name, &country.iso2),
                    is_india: is_india(country),
                });
            }
        }
    }

    for (country_id, ring_id) in index.polygon_candidates(lat, lon) {
        if !allowed_countries.is_empty() && !allowed_countries.contains(&country_id) {
            continue;
        }

        let Some(country) = countries.get(country_id as usize) else {
            continue;
        };
        let Some(ring) = country.polygons.get(ring_id as usize) else {
            continue;
        };

        if crate::engine::polygon::point_in_ring(lat, lon, ring) {
            return Ok(CountryLookup {
                region: region_from_archived(&country.name, &country.iso2),
                is_india: is_india(country),
            });
        }
    }

    let country = candidates
        .into_iter()
        .find_map(|id| {
            let country = countries.get(id as usize)?;
            let inside = country
                .polygons
                .iter()
                .any(|ring| crate::engine::polygon::point_in_ring(lat, lon, ring));
            if inside { Some(country) } else { None }
        })
        .ok_or(GeoEngineError::CountryNotFound { lat, lon })?;

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

#[derive(Clone)]
struct SubdistrictMetadata {
    subdistrict_name: String,
    district_name: String,
    state_name: String,
    subdistrict_code: String,
    district_code: String,
    state_code: String,
    demographics: Option<DistrictDemographics>,
}

fn load_subdistrict_meta(path: &Path) -> Result<Vec<SubdistrictMetadata>, GeoEngineError> {
    let bytes = fs::read(path).map_err(|source| GeoEngineError::DatabaseOpen {
        path: path.to_path_buf(),
        source,
    })?;
    load_subdistrict_meta_from_bytes(&bytes)
}

fn load_subdistrict_meta_from_bytes(
    bytes: &[u8],
) -> Result<Vec<SubdistrictMetadata>, GeoEngineError> {
    let decoded = if is_zstd_blob(bytes) {
        zstd::stream::decode_all(bytes).map_err(|source| GeoEngineError::DatabaseMap {
            path: PathBuf::from("subdistrict.meta"),
            source,
        })?
    } else {
        bytes.to_vec()
    };

    let archived_meta: &Archived<SubdistrictMeta> =
        rkyv::access::<Archived<SubdistrictMeta>, rkyv::rancor::Error>(&decoded)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(&decoded) });

    let mut metadata = Vec::with_capacity(archived_meta.entries.len());
    for entry in archived_meta.entries.iter() {
        let languages = entry
            .languages_blob_id
            .as_ref()
            .map(|id| subdistrict_meta_string(archived_meta, u32::from(*id) as usize))
            .transpose()?
            .unwrap_or_default();

        let demographics = if entry.district_uni_code_id.is_none()
            && entry.major_religion_id.is_none()
            && languages.is_empty()
        {
            None
        } else {
            Some(DistrictDemographics {
                district_uni_code: entry
                    .district_uni_code_id
                    .as_ref()
                    .map(|id| subdistrict_meta_string(archived_meta, u32::from(*id) as usize))
                    .transpose()?
                    .unwrap_or_default(),
                major_religion: entry
                    .major_religion_id
                    .as_ref()
                    .map(|id| subdistrict_meta_string(archived_meta, u32::from(*id) as usize))
                    .transpose()?
                    .unwrap_or_default(),
                languages: parse_embedded_languages(&languages),
            })
        };

        metadata.push(SubdistrictMetadata {
            subdistrict_name: normalize_name(&subdistrict_meta_string(
                archived_meta,
                u32::from(entry.subdistrict_name_id) as usize,
            )?),
            district_name: normalize_name(&subdistrict_meta_string(
                archived_meta,
                u32::from(entry.district_name_id) as usize,
            )?),
            state_name: normalize_name(&subdistrict_meta_string(
                archived_meta,
                u32::from(entry.state_name_id) as usize,
            )?),
            subdistrict_code: subdistrict_meta_string(
                archived_meta,
                u32::from(entry.subdistrict_code_id) as usize,
            )?,
            district_code: subdistrict_meta_string(
                archived_meta,
                u32::from(entry.district_code_id) as usize,
            )?,
            state_code: subdistrict_meta_string(
                archived_meta,
                u32::from(entry.state_code_id) as usize,
            )?,
            demographics,
        });
    }

    Ok(metadata)
}

fn subdistrict_meta_string(
    meta: &Archived<SubdistrictMeta>,
    id: usize,
) -> Result<String, GeoEngineError> {
    meta.strings
        .get(id)
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| GeoEngineError::DatabaseMap {
            path: PathBuf::from("subdistrict.meta"),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid subdistrict metadata string id: {id}"),
            ),
        })
}

fn resolve_subdistrict_metadata(
    feature_name: &str,
    metadata: Option<&[SubdistrictMetadata]>,
) -> Option<SubdistrictMetadata> {
    if let Some(index) = parse_subdistrict_meta_key(feature_name)
        && let Some(metadata) = metadata
    {
        return metadata.get(index).cloned();
    }

    parse_subdistrict_payload(feature_name)
}

fn parse_subdistrict_meta_key(value: &str) -> Option<usize> {
    value
        .strip_prefix("sdm:")
        .and_then(|id| id.parse::<usize>().ok())
}

fn is_zstd_blob(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == [0x28, 0xB5, 0x2F, 0xFD]
}

fn parse_subdistrict_payload(payload: &str) -> Option<SubdistrictMetadata> {
    let parts: Vec<&str> = payload.split(SUBDISTRICT_FIELD_SEPARATOR).collect();
    if parts.len() != 6 && parts.len() != 8 && parts.len() != 9 {
        return None;
    }

    Some(SubdistrictMetadata {
        subdistrict_name: normalize_name(parts[0].trim()),
        district_name: normalize_name(parts[1].trim()),
        state_name: normalize_name(parts[2].trim()),
        subdistrict_code: parts[3].trim().to_string(),
        district_code: parts[4].trim().to_string(),
        state_code: parts[5].trim().to_string(),
        demographics: parse_embedded_demographics(&parts),
    })
}

fn parse_embedded_demographics(parts: &[&str]) -> Option<DistrictDemographics> {
    if parts.len() < 8 {
        return None;
    }

    let (district_uni_code, major_religion_idx, languages_idx) = if parts.len() >= 9 {
        (parts[6].trim().to_string(), 7usize, 8usize)
    } else {
        (parts[4].trim().to_string(), 6usize, 7usize)
    };
    let major_religion = parts[major_religion_idx].trim().to_string();
    let languages = parse_embedded_languages(parts[languages_idx].trim());
    if district_uni_code.is_empty() && major_religion.is_empty() && languages.is_empty() {
        return None;
    }

    Some(DistrictDemographics {
        district_uni_code,
        major_religion,
        languages,
    })
}

fn parse_embedded_languages(raw: &str) -> Vec<GeoLanguage> {
    if raw.is_empty() {
        return Vec::new();
    }

    let langs: Vec<GeoLanguage> = raw
        .split(LANGUAGE_ENTRY_SEPARATOR)
        .filter_map(|entry| {
            let mut parts = entry.split(LANGUAGE_COMPONENT_SEPARATOR);
            let name = parts.next()?.trim();
            let usage_type = parts.next()?.trim();
            let code = parts.next()?.trim();

            if name.is_empty() {
                return None;
            }

            Some(GeoLanguage {
                code: code.to_string(),
                name: name.to_string(),
                usage_type: usage_type.to_string(),
            })
        })
        .collect();

    // 🔥 sort by relevance before returning
    sort_languages_by_relevance(&langs)
}

fn load_cities_by_id(
    core_path: &Path,
    meta_path: &Path,
) -> Result<HashMap<u32, City>, GeoEngineError> {
    let core_bytes = fs::read(core_path).map_err(|source| GeoEngineError::DatabaseOpen {
        path: core_path.to_path_buf(),
        source,
    })?;
    let meta_bytes = fs::read(meta_path).map_err(|source| GeoEngineError::DatabaseOpen {
        path: meta_path.to_path_buf(),
        source,
    })?;
    load_cities_by_id_from_bytes(&core_bytes, &meta_bytes)
}

fn load_cities_by_id_from_bytes(
    core_bytes: &[u8],
    meta_bytes: &[u8],
) -> Result<HashMap<u32, City>, GeoEngineError> {
    let archived_core: &Archived<Vec<CityCore>> =
        rkyv::access::<Archived<Vec<CityCore>>, rkyv::rancor::Error>(core_bytes).unwrap_or_else(
            |_| unsafe {
                // SAFETY: rkyv data layout is guaranteed valid even if validation fails.
                // Using unchecked access as fallback after failed validated check.
                rkyv::access_unchecked(core_bytes)
            },
        );

    let archived_meta: &Archived<CityMeta> =
        rkyv::access::<Archived<CityMeta>, rkyv::rancor::Error>(meta_bytes).unwrap_or_else(|_| {
            unsafe {
                // SAFETY: rkyv data layout is guaranteed valid even if validation fails.
                // Using unchecked access as fallback after failed validated check.
                rkyv::access_unchecked(meta_bytes)
            }
        });

    let mut cities = HashMap::with_capacity(archived_core.len());
    for archived_city in archived_core.iter() {
        let city = City {
            geoname_id: archived_city.geoname_id.into(),
            country_code: city_string(
                archived_meta,
                u32::from(archived_city.country_code_id) as usize,
            )?,
            name: city_string(archived_meta, u32::from(archived_city.name_id) as usize)?,
            ascii: city_string(archived_meta, u32::from(archived_city.ascii_id) as usize)?,
            admin1_code: city_optional_string(
                archived_meta,
                archived_city
                    .admin1_code_id
                    .as_ref()
                    .map(|value| u32::from(*value)),
            )?,
            admin1_name: city_optional_string(
                archived_meta,
                archived_city
                    .admin1_name_id
                    .as_ref()
                    .map(|value| u32::from(*value)),
            )?,
            admin2_code: city_optional_string(
                archived_meta,
                archived_city
                    .admin2_code_id
                    .as_ref()
                    .map(|value| u32::from(*value)),
            )?,
            admin2_name: city_optional_string(
                archived_meta,
                archived_city
                    .admin2_name_id
                    .as_ref()
                    .map(|value| u32::from(*value)),
            )?,
            lat: archived_city.lat.into(),
            lon: archived_city.lon.into(),
        };

        cities.insert(city.geoname_id, city);
    }

    Ok(cities)
}

fn city_string(meta: &Archived<CityMeta>, id: usize) -> Result<String, GeoEngineError> {
    meta.strings
        .get(id)
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| GeoEngineError::DatabaseMap {
            path: PathBuf::from("cities.meta"),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid city string id: {id}"),
            ),
        })
}

fn city_optional_string(
    meta: &Archived<CityMeta>,
    id: Option<u32>,
) -> Result<Option<String>, GeoEngineError> {
    match id {
        Some(inner) => Ok(Some(city_string(meta, inner as usize)?)),
        None => Ok(None),
    }
}

fn calculate_polygon_center(polygons: &Archived<Vec<Vec<(f32, f32)>>>) -> (f32, f32) {
    if polygons.is_empty() {
        return (0.0, 0.0);
    }

    // Use the first (main) polygon
    let polygon = &polygons[0];
    if polygon.is_empty() {
        return (0.0, 0.0);
    }

    // Calculate centroid of the polygon
    let mut sum_lat = 0.0f32;
    let mut sum_lon = 0.0f32;
    let count = polygon.len() as f32;

    for point in polygon.iter() {
        // Coordinates are stored as (lon, lat) based on the point_in_ring function
        let lon_val: f32 = point.0.into();
        let lat_val: f32 = point.1.into();
        sum_lat += lat_val;
        sum_lon += lon_val;
    }

    let center_lat = sum_lat / count;
    let center_lon = sum_lon / count;

    (center_lat, center_lon)
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

fn sort_languages_by_relevance(languages: &[GeoLanguage]) -> Vec<GeoLanguage> {
    let mut langs = languages.to_vec();

    // Assign priority weight
    fn weight(usage: &str) -> u8 {
        if usage.eq_ignore_ascii_case("primary") {
            0
        } else if usage.eq_ignore_ascii_case("major") {
            1
        } else if usage.eq_ignore_ascii_case("administrative") {
            2
        } else {
            3
        }
    }

    // Stable sort by weight
    langs.sort_by_key(|l| weight(&l.usage_type));

    // Return language codes in order
    langs
}

