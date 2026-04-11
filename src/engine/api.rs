use std::path::{Path, PathBuf};

use rkyv::{Archived, string::ArchivedString};

use crate::district_data::GeoLanguage;
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
    pub demographics: Option<DistrictDemographics>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubdistrictMatch {
    pub subdistrict: Region,
    pub district: Region,
    pub state: Region,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictDemographics {
    pub district_uni_code: String,
    pub major_religion: String,
    pub languages: Vec<GeoLanguage>,
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
    country_engine: GeoEngine,
    subdistrict_engine: Option<GeoEngine>,
    subdistrict_db_path: PathBuf,
}

pub fn lookup_with_subdistrict_path(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    subdistrict_db_path: Option<&Path>,
) -> Result<LookupResult, GeoEngineError> {
    let engine = InitializedGeoEngine::open(country_db_path, subdistrict_db_path)?;
    engine.lookup(lat, lon)
}

pub fn lookup_address_details_with_subdistrict_path(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    subdistrict_db_path: Option<&Path>,
) -> Result<AddressDetails, GeoEngineError> {
    let engine = InitializedGeoEngine::open(country_db_path, subdistrict_db_path)?;
    engine.lookup_address_details(lat, lon)
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

impl InitializedGeoEngine {
    pub fn open(
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
            country_engine,
            subdistrict_engine,
            subdistrict_db_path: resolved_subdistrict_path,
        })
    }

    pub fn lookup(&self, lat: f32, lon: f32) -> Result<LookupResult, GeoEngineError> {
        let country = lookup_country(lat, lon, &self.country_engine)?;

        if !country.is_india {
            return Ok(LookupResult {
                country: country.region,
                state: None,
                district: None,
                subdistrict: None,
                demographics: None,
            });
        }

        let Some(subdistrict_engine) = self.subdistrict_engine.as_ref() else {
            return Err(GeoEngineError::DistrictDatabaseUnavailable {
                path: self.subdistrict_db_path.clone(),
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
            demographics: metadata.demographics,
        });
    }

    Ok(LookupResult {
        country,
        state: None,
        district: None,
        subdistrict: subdistrict_match,
        demographics: None,
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
            .map(|demographics| demographics.district_uni_code.clone()),
        subdistrict: result.subdistrict,
        major_religion: result
            .demographics
            .as_ref()
            .map(|demographics| demographics.major_religion.clone()),
        languages: result
            .demographics
            .map(|demographics| demographics.languages)
            .unwrap_or_default(),
    }
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


fn sort_languages_by_relevance(
    languages: &[GeoLanguage],
) -> Vec<GeoLanguage> {
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