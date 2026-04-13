use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{
    collections::{BTreeSet, HashMap},
    fs,
};

use fst::{IntoStreamer, Map, Streamer};
use rkyv::{Archived, string::ArchivedString};

use crate::district_data::GeoLanguage;
use crate::engine::city::{City, normalize};
use crate::engine::error::GeoEngineError;
use crate::engine::model::Country;
use crate::engine::{index::SpatialIndex, lookup::find_country, runtime::GeoEngine};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub name: String,
    pub iso2: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LookupResult {
    pub country: Region,
    pub state: Option<Region>,
    pub district: Option<Region>,
    pub subdistrict: Option<Region>,
    pub demographics: Option<DistrictDemographics>,
    pub latitude: f32,
    pub longitude: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictDemographics {
    pub district_uni_code: String,
    pub major_religion: String,
    pub languages: Vec<GeoLanguage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubdistrictMatch {
    pub subdistrict: Region,
    pub district: Region,
    pub state: Region,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CityMatch {
    pub geoname_id: u32,
    pub name: String,
    pub ascii: String,
    pub country_code: String,
    pub admin1_name: Option<String>,
    pub admin1_code: Option<String>,
    pub admin2_name: Option<String>,
    pub admin2_code: Option<String>,
    pub latitude: f32,
    pub longitude: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CombinedSearchResult {
    pub cities: Vec<CityMatch>,
    pub subdistricts: Vec<SubdistrictMatch>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReverseGeocodingResult {
    pub country: Region,
    pub state: Option<Region>,
    pub district: Option<Region>,
    pub subdistrict: Option<Region>,
    pub city: CityMatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressDetails {
    pub full_address: String,
    pub country: Region,
    pub state: Option<Region>,
    pub district: Option<Region>,
    pub district_uni_code: Option<String>,
    pub subdistrict: Option<Region>,
    pub major_religion: Option<String>,
    pub languages: Vec<GeoLanguage>,
}

pub struct InitializedGeoEngine {
    engine: EngineBundle,
}

struct EngineBundle {
    country: GeoEngine,
    subdistrict: Option<GeoEngine>,
    subdistrict_db_path: PathBuf,
    city_index: Option<CityIndex>,
}

struct CityIndex {
    fst: Option<Map<Vec<u8>>>,
    cities_by_id: HashMap<u32, City>,
    city_rkyv_path: PathBuf,
}

struct InitializedPaths {
    country_db_path: PathBuf,
    subdistrict_db_path: PathBuf,
    city_fst_path: PathBuf,
    city_rkyv_path: PathBuf,
}

static PATHS: OnceLock<InitializedPaths> = OnceLock::new();
static ENGINE: OnceLock<Result<InitializedGeoEngine, String>> = OnceLock::new();

pub fn init_path(
    country_db_path: &Path,
    subdistrict_db_path: &Path,
    city_fst_path: &Path,
    city_rkyv_path: &Path,
) -> Result<(), GeoEngineError> {
    let initialized_paths = PATHS.get_or_init(|| InitializedPaths {
        country_db_path: country_db_path.to_path_buf(),
        subdistrict_db_path: subdistrict_db_path.to_path_buf(),
        city_fst_path: city_fst_path.to_path_buf(),
        city_rkyv_path: city_rkyv_path.to_path_buf(),
    });

    let same_paths = initialized_paths.country_db_path == country_db_path
        && initialized_paths.subdistrict_db_path == subdistrict_db_path
        && initialized_paths.city_fst_path == city_fst_path
        && initialized_paths.city_rkyv_path == city_rkyv_path;

    if !same_paths {
        return Err(GeoEngineError::PathsAlreadyInitialized);
    }

    let _ = get_initialized_engine()?;
    Ok(())
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
            paths.city_fst_path.as_path(),
            paths.city_rkyv_path.as_path(),
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

pub fn reverse_geocoding(lat: f32, lon: f32) -> Result<ReverseGeocodingResult, GeoEngineError> {
    let engine = get_initialized_engine()?;
    engine.reverse_geocoding(lat, lon)
}

pub fn search(query: &str) -> Result<CombinedSearchResult, GeoEngineError> {
    let engine = get_initialized_engine()?;
    engine.search_places_by_name(query, None)
}

pub fn lookup_with_subdistrict_path(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    subdistrict_db_path: Option<&Path>,
) -> Result<LookupResult, GeoEngineError> {
    let engine = InitializedGeoEngine::open_lookup_only(country_db_path, subdistrict_db_path)?;
    engine.lookup(lat, lon)
}

pub fn lookup_address_details_with_subdistrict_path(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    subdistrict_db_path: Option<&Path>,
) -> Result<AddressDetails, GeoEngineError> {
    let engine = InitializedGeoEngine::open_lookup_only(country_db_path, subdistrict_db_path)?;
    engine.lookup_address_details(lat, lon)
}


pub fn reverse_geocoding_with_paths(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    subdistrict_db_path: &Path,
    city_rkyv_path: &Path,
) -> Result<ReverseGeocodingResult, GeoEngineError> {
    let engine = InitializedGeoEngine::open_lookup_only(country_db_path, Some(subdistrict_db_path))?;
    let city = nearest_city(city_rkyv_path, lat, lon)?;
    let lookup = engine.lookup(lat, lon)?;

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

pub fn search_subdistricts_by_name(
    query: &str,
    subdistrict_db_path: &Path,
) -> Result<Vec<SubdistrictMatch>, GeoEngineError> {
    let normalized_query = query.trim();
    if normalized_query.is_empty() {
        return Ok(Vec::new());
    }

    let engine = open_subdistrict_engine(subdistrict_db_path)?;
    let query_lower = normalized_query.to_lowercase();
    let mut matches = Vec::new();

    for feature in engine.countries().iter() {
        let Some(metadata) = parse_subdistrict_payload(feature.name.as_str()) else {
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

pub fn search_cities_by_name(
    query: &str,
    city_fst_path: &Path,
    city_rkyv_path: &Path,
    limit: usize,
) -> Result<Vec<CityMatch>, GeoEngineError> {
    search_cities_by_name_internal(query, city_fst_path, city_rkyv_path, Some(limit))
}

pub fn search_by_name(
    query: &str,
    subdistrict_db_path: &Path,
    city_fst_path: &Path,
    city_rkyv_path: &Path,
) -> Result<CombinedSearchResult, GeoEngineError> {
    let subdistricts = search_subdistricts_by_name(query, subdistrict_db_path)?;
    let cities = search_cities_by_name_internal(query, city_fst_path, city_rkyv_path, None)?;

    Ok(CombinedSearchResult {
        cities,
        subdistricts,
    })
}

fn search_cities_by_name_internal(
    query: &str,
    city_fst_path: &Path,
    city_rkyv_path: &Path,
    limit: Option<usize>,
) -> Result<Vec<CityMatch>, GeoEngineError> {
    let normalized = normalize(query.trim());
    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let fst_bytes = fs::read(city_fst_path).map_err(|source| GeoEngineError::DatabaseOpen {
        path: city_fst_path.to_path_buf(),
        source,
    })?;
    let fst = Map::new(fst_bytes).map_err(|err| GeoEngineError::DatabaseMap {
        path: city_fst_path.to_path_buf(),
        source: std::io::Error::other(err.to_string()),
    })?;

    let cities_by_id = load_cities_by_id(city_rkyv_path)?;

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
            cities_by_id.get(&geoname_id).map(|city| CityMatch {
                geoname_id: city.geoname_id,
                name: city.name.clone(),
                ascii: city.ascii.clone(),
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

pub fn search_places_by_name(
    query: &str,
    subdistrict_db_path: &Path,
    city_fst_path: &Path,
    city_rkyv_path: &Path,
    city_limit: usize,
) -> Result<CombinedSearchResult, GeoEngineError> {
    let subdistricts = search_subdistricts_by_name(query, subdistrict_db_path)?;
    let cities =
        search_cities_by_name_internal(query, city_fst_path, city_rkyv_path, Some(city_limit))?;

    Ok(CombinedSearchResult {
        cities,
        subdistricts,
    })
}

impl InitializedGeoEngine {
    pub fn open(
        country_db_path: &Path,
        subdistrict_db_path: &Path,
        city_fst_path: &Path,
        city_rkyv_path: &Path,
    ) -> Result<Self, GeoEngineError> {
        let country_engine = GeoEngine::open(country_db_path)?;
        let subdistrict_engine = Some(open_subdistrict_engine(subdistrict_db_path)?);
        let city_index = load_city_index(Some(city_fst_path), Some(city_rkyv_path))?;

        Ok(Self {
            engine: EngineBundle {
                country: country_engine,
                subdistrict: subdistrict_engine,
                subdistrict_db_path: subdistrict_db_path.to_path_buf(),
                city_index,
            },
        })
    }

    fn open_lookup_only(
        country_db_path: &Path,
        subdistrict_db_path: Option<&Path>,
    ) -> Result<Self, GeoEngineError> {
        let country_engine = GeoEngine::open(country_db_path)?;
        let resolved_subdistrict_path =
            resolve_subdistrict_path(country_db_path, subdistrict_db_path);
        let subdistrict_engine = if resolved_subdistrict_path.exists() {
            Some(open_subdistrict_engine(&resolved_subdistrict_path)?)
        } else {
            None
        };

        Ok(Self {
            engine: EngineBundle {
                country: country_engine,
                subdistrict: subdistrict_engine,
                subdistrict_db_path: resolved_subdistrict_path,
                city_index: None,
            },
        })
    }

    pub fn lookup(&self, lat: f32, lon: f32) -> Result<LookupResult, GeoEngineError> {
        let country = lookup_country(lat, lon, &self.engine.country)?;

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

        lookup_india_with_subdistrict_engine(lat, lon, country.region, subdistrict_engine)
    }

    pub fn lookup_address_details(
        &self,
        lat: f32,
        lon: f32,
    ) -> Result<AddressDetails, GeoEngineError> {
        let result = self.lookup(lat, lon)?;
        Ok(address_details_from_lookup(result))
    }

    pub fn reverse_geocoding(&self, lat: f32, lon: f32) -> Result<ReverseGeocodingResult, GeoEngineError> {
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
            let Some(metadata) = parse_subdistrict_payload(feature.name.as_str()) else {
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
                city_index.cities_by_id.get(&geoname_id).map(|city| CityMatch {
                    geoname_id: city.geoname_id,
                    name: city.name.clone(),
                    ascii: city.ascii.clone(),
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
                path: PathBuf::from("city.rkyv"),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "city dataset not initialized",
                ),
            });
        };

        nearest_city_from_map(lat, lon, &city_index.cities_by_id, &city_index.city_rkyv_path)
    }
}

fn lookup_india_with_subdistrict_engine(
    lat: f32,
    lon: f32,
    country: Region,
    subdistrict_engine: &GeoEngine,
) -> Result<LookupResult, GeoEngineError> {
    let subdistrict_index = SpatialIndex::build(subdistrict_engine.countries());
    let subdistrict_match_with_center = match find_country(lat, lon, &subdistrict_index) {
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
        && let Some(metadata) = parse_subdistrict_payload(&subdistrict_match.name)
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
        country,
        state: None,
        district: None,
        subdistrict: subdistrict_match_with_center.map(|(region, _)| region),
        demographics: None,
        latitude: fallback_lat,
        longitude: fallback_lon,
    })
}

fn address_details_from_lookup(result: LookupResult) -> AddressDetails {
    let full_address = format_full_address(
        result.subdistrict.as_ref(),
        result.district.as_ref(),
        result.state.as_ref(),
        Some(&result.country),
    );

    AddressDetails {
        full_address,
        country: result.country,
        state: result.state,
        district: result.district,
        district_uni_code: result
            .demographics
            .as_ref()
            .map(|d| d.district_uni_code.clone()),
        subdistrict: result.subdistrict,
        major_religion: result
            .demographics
            .as_ref()
            .map(|d| d.major_religion.clone()),
        languages: result.demographics.map(|d| d.languages).unwrap_or_default(),
    }
}

fn nearest_city_from_map(
    lat: f32,
    lon: f32,
    cities_by_id: &HashMap<u32, City>,
    city_rkyv_path: &Path,
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
            path: city_rkyv_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "no cities found in city dataset",
            ),
        })?;

    Ok(CityMatch {
        geoname_id: nearest.geoname_id,
        name: nearest.name.clone(),
        ascii: nearest.ascii.clone(),
        country_code: nearest.country_code.clone(),
        admin1_name: nearest.admin1_name.clone(),
        admin1_code: nearest.admin1_code.clone(),
        admin2_name: nearest.admin2_name.clone(),
        admin2_code: nearest.admin2_code.clone(),
        latitude: nearest.lat,
        longitude: nearest.lon,
    })
}

fn nearest_city(city_rkyv_path: &Path, lat: f32, lon: f32) -> Result<CityMatch, GeoEngineError> {
    let cities_by_id = load_cities_by_id(city_rkyv_path)?;
    nearest_city_from_map(lat, lon, &cities_by_id, city_rkyv_path)
}

fn load_city_index(
    city_fst_path: Option<&Path>,
    city_rkyv_path: Option<&Path>,
) -> Result<Option<CityIndex>, GeoEngineError> {
    let Some(city_rkyv_path) = city_rkyv_path else {
        return Ok(None);
    };

    let cities_by_id = load_cities_by_id(city_rkyv_path)?;
    let fst = if let Some(city_fst_path) = city_fst_path {
        let fst_bytes = fs::read(city_fst_path).map_err(|source| GeoEngineError::DatabaseOpen {
            path: city_fst_path.to_path_buf(),
            source,
        })?;
        Some(Map::new(fst_bytes).map_err(|err| GeoEngineError::DatabaseMap {
            path: city_fst_path.to_path_buf(),
            source: std::io::Error::other(err.to_string()),
        })?)
    } else {
        None
    };

    Ok(Some(CityIndex {
        fst,
        cities_by_id,
        city_rkyv_path: city_rkyv_path.to_path_buf(),
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
    demographics: Option<DistrictDemographics>,
}

fn parse_subdistrict_payload(payload: &str) -> Option<SubdistrictMetadata> {
    let parts: Vec<&str> = payload.split("||").collect();
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
        .split("##")
        .filter_map(|entry| {
            let mut parts = entry.split("~~");
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

fn load_cities_by_id(path: &Path) -> Result<HashMap<u32, City>, GeoEngineError> {
    let bytes = fs::read(path).map_err(|source| GeoEngineError::DatabaseOpen {
        path: path.to_path_buf(),
        source,
    })?;
    let archived: &Archived<Vec<City>> = rkyv::access::<Archived<Vec<City>>, rkyv::rancor::Error>(
        &bytes,
    )
    .unwrap_or_else(|_| unsafe {
        // SAFETY: rkyv data layout is guaranteed valid even if validation fails.
        // Using unchecked access as fallback after failed validated check.
        rkyv::access_unchecked(&bytes)
    });

    let mut cities = HashMap::with_capacity(archived.len());
    for archived_city in archived.iter() {
        let city = City {
            geoname_id: archived_city.geoname_id.into(),
            country_code: archived_city.country_code.as_str().to_string(),
            name: archived_city.name.as_str().to_string(),
            ascii: archived_city.ascii.as_str().to_string(),
            alternates: archived_city
                .alternates
                .iter()
                .map(|value| value.as_str().to_string())
                .collect(),
            admin1_code: archived_city
                .admin1_code
                .as_ref()
                .map(|v| v.as_str().to_string()),
            admin1_name: archived_city
                .admin1_name
                .as_ref()
                .map(|v| v.as_str().to_string()),
            admin2_code: archived_city
                .admin2_code
                .as_ref()
                .map(|v| v.as_str().to_string()),
            admin2_name: archived_city
                .admin2_name
                .as_ref()
                .map(|v| v.as_str().to_string()),
            lat: archived_city.lat.into(),
            lon: archived_city.lon.into(),
        };

        cities.insert(city.geoname_id, city);
    }

    Ok(cities)
}

fn format_full_address(
    subdistrict: Option<&Region>,
    district: Option<&Region>,
    state: Option<&Region>,
    country: Option<&Region>,
) -> String {
    let mut parts = Vec::new();

    if let Some(region) = subdistrict {
        parts.push(region.name.as_str());
    }
    if let Some(region) = district {
        parts.push(region.name.as_str());
    }
    if let Some(region) = state {
        parts.push(region.name.as_str());
    }
    if let Some(region) = country {
        parts.push(region.name.as_str());
    }

    parts.join(", ")
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
