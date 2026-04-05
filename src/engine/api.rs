use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rkyv::{Archived, string::ArchivedString};
use sha2::{Digest, Sha256};

use crate::engine::error::GeoEngineError;
use crate::engine::model::Country;
use crate::engine::{index::SpatialIndex, lookup::find_country, runtime::GeoEngine};

static GLOBAL_ENGINES: OnceLock<EngineSet> = OnceLock::new();
const GITHUB_RAW_BASE_URL: &str =
    "https://raw.githubusercontent.com/sumitsharansatsangi/geo_engine/main";

struct EngineSet {
    country: GeoEngine,
    subdistrict: GeoEngine,
}

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

pub fn lookup_place(lat: f32, lon: f32) -> Result<String, GeoEngineError> {
    let result = lookup(lat, lon)?;
    Ok(format_place(&result))
}

pub fn lookup(lat: f32, lon: f32) -> Result<LookupResult, GeoEngineError> {
    let engines = ensure_global_engines()?;
    let country = lookup_country(lat, lon, &engines.country)?;

    if !country.is_india {
        return Ok(LookupResult {
            country: country.region,
            state: None,
            district: None,
            subdistrict: None,
        });
    }

    lookup_india_with_subdistrict_engine(lat, lon, country.region, &engines.subdistrict)
}

pub fn lookup_with_paths(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    _state_db_path: Option<&Path>,
) -> Result<LookupResult, GeoEngineError> {
    lookup_with_subdistrict_path(lat, lon, country_db_path, None)
}

pub fn lookup_with_district_path(
    lat: f32,
    lon: f32,
    country_db_path: &Path,
    _state_db_path: Option<&Path>,
    district_db_path: Option<&Path>,
) -> Result<LookupResult, GeoEngineError> {
    lookup_with_subdistrict_path(lat, lon, country_db_path, district_db_path)
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

pub fn init_databases(country_db_path: &Path, subdistrict_db_path: &Path) -> Result<(), GeoEngineError> {
    let engines = EngineSet {
        country: GeoEngine::open(country_db_path)?,
        subdistrict: open_subdistrict_engine(subdistrict_db_path)?,
    };

    match GLOBAL_ENGINES.set(engines) {
        Ok(()) => Ok(()),
        Err(_) => Ok(()),
    }
}

pub fn init_databases_from_strings(
    country_db_path: String,
    subdistrict_db_path: String,
) -> Result<(), GeoEngineError> {
    init_databases(Path::new(&country_db_path), Path::new(&subdistrict_db_path))
}

pub fn init_with_remote(cache_dir: &Path) -> Result<(), GeoEngineError> {
    fs::create_dir_all(cache_dir).map_err(|source| GeoEngineError::DatabaseOpen {
        path: cache_dir.to_path_buf(),
        source,
    })?;

    let country_path = cache_dir.join("geo.db");
    let subdistrict_path = cache_dir.join("subdistrict.db");
    let country_checksum_path = cache_dir.join("geo.db.sha256");
    let subdistrict_checksum_path = cache_dir.join("subdistrict.db.sha256");
    let base_url =
        std::env::var("GEO_ENGINE_DB_BASE_URL").unwrap_or_else(|_| GITHUB_RAW_BASE_URL.to_string());

    sync_file_with_remote(
        &country_path,
        &country_checksum_path,
        &format!("{base_url}/geo.db"),
        &format!("{base_url}/geo.db.sha256"),
    )?;
    sync_file_with_remote(
        &subdistrict_path,
        &subdistrict_checksum_path,
        &format!("{base_url}/subdistrict.db"),
        &format!("{base_url}/subdistrict.db.sha256"),
    )?;

    init_databases(&country_path, &subdistrict_path)
}

pub fn init_with_remote_path(cache_dir: String) -> Result<(), GeoEngineError> {
    init_with_remote(Path::new(&cache_dir))
}

fn open_subdistrict_engine(subdistrict_db_path: &Path) -> Result<GeoEngine, GeoEngineError> {
    GeoEngine::open(subdistrict_db_path).map_err(|err| match err {
        GeoEngineError::DatabaseOpen { source, .. }
        | GeoEngineError::DatabaseMap { source, .. } => GeoEngineError::DistrictDatabaseUnavailable {
            path: PathBuf::from(subdistrict_db_path),
            source,
        },
        other => other,
    })
}

fn ensure_global_engines() -> Result<&'static EngineSet, GeoEngineError> {
    if let Some(engines) = GLOBAL_ENGINES.get() {
        return Ok(engines);
    }
    Err(GeoEngineError::DatabaseOpen {
        path: PathBuf::from("<not initialized>"),
        source: io::Error::new(
            io::ErrorKind::NotFound,
            "geo_engine is not initialized; call init_databases(...), init_databases_from_strings(...), init_with_remote(...), or init_with_remote_path(...) first",
        ),
    })
}

fn sync_file_with_remote(
    local_path: &Path,
    local_checksum_path: &Path,
    remote_file_url: &str,
    remote_checksum_url: &str,
) -> Result<(), GeoEngineError> {
    let local_available = has_nonempty_file(local_path)?;
    let expected_checksum = match fetch_remote_checksum(remote_checksum_url, local_path) {
        Ok(v) => v,
        Err(err) => {
            if local_available {
                return Ok(());
            }
            return Err(err);
        }
    };
    let current_checksum = file_sha256(local_path)?;

    if let Some(current) = current_checksum {
        if current.eq_ignore_ascii_case(&expected_checksum) {
            write_checksum_file(local_checksum_path, &expected_checksum)?;
            return Ok(());
        }
    }

    let body = match download_bytes(remote_file_url, local_path) {
        Ok(v) => v,
        Err(err) => {
            if local_available {
                return Ok(());
            }
            return Err(err);
        }
    };
    let downloaded_checksum = sha256_hex(&body);
    if !downloaded_checksum.eq_ignore_ascii_case(&expected_checksum) {
        if local_available {
            return Ok(());
        }
        return Err(GeoEngineError::DatabaseOpen {
            path: local_path.to_path_buf(),
            source: io::Error::other(format!(
                "checksum mismatch for {remote_file_url}: expected {expected_checksum}, got {downloaded_checksum}"
            )),
        });
    }

    let parent = local_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| GeoEngineError::DatabaseOpen {
        path: parent.to_path_buf(),
        source,
    })?;

    let tmp_path = local_path.with_extension("part");
    fs::write(&tmp_path, &body).map_err(|source| GeoEngineError::DatabaseOpen {
        path: tmp_path.clone(),
        source,
    })?;
    fs::rename(&tmp_path, local_path).map_err(|source| GeoEngineError::DatabaseOpen {
        path: local_path.to_path_buf(),
        source,
    })?;
    write_checksum_file(local_checksum_path, &expected_checksum)?;
    Ok(())
}

fn fetch_remote_checksum(remote_checksum_url: &str, local_path: &Path) -> Result<String, GeoEngineError> {
    let text = download_text(remote_checksum_url, local_path)?;
    parse_checksum(&text).ok_or_else(|| GeoEngineError::DatabaseOpen {
        path: local_path.to_path_buf(),
        source: io::Error::other(format!(
            "invalid checksum file at {remote_checksum_url}: expected a SHA-256 hash"
        )),
    })
}

fn download_text(url: &str, local_path: &Path) -> Result<String, GeoEngineError> {
    let response = reqwest::blocking::get(url).map_err(|source| GeoEngineError::DatabaseOpen {
        path: local_path.to_path_buf(),
        source: io::Error::other(format!("download failed from {url}: {source}")),
    })?;

    if !response.status().is_success() {
        return Err(GeoEngineError::DatabaseOpen {
            path: local_path.to_path_buf(),
            source: io::Error::other(format!(
                "download failed from {url}: HTTP {}",
                response.status()
            )),
        });
    }

    response.text().map_err(|source| GeoEngineError::DatabaseOpen {
        path: local_path.to_path_buf(),
        source: io::Error::other(format!("failed to read text body from {url}: {source}")),
    })
}

fn download_bytes(url: &str, local_path: &Path) -> Result<Vec<u8>, GeoEngineError> {
    let response = reqwest::blocking::get(url).map_err(|source| GeoEngineError::DatabaseOpen {
        path: local_path.to_path_buf(),
        source: io::Error::other(format!("download failed from {url}: {source}")),
    })?;

    if !response.status().is_success() {
        return Err(GeoEngineError::DatabaseOpen {
            path: local_path.to_path_buf(),
            source: io::Error::other(format!(
                "download failed from {url}: HTTP {}",
                response.status()
            )),
        });
    }

    response
        .bytes()
        .map(|b| b.to_vec())
        .map_err(|source| GeoEngineError::DatabaseOpen {
            path: local_path.to_path_buf(),
            source: io::Error::other(format!("failed to read binary body from {url}: {source}")),
        })
}

fn parse_checksum(text: &str) -> Option<String> {
    let token = text.split_whitespace().next()?;
    if token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(token.to_ascii_lowercase())
    } else {
        None
    }
}

fn file_sha256(path: &Path) -> Result<Option<String>, GeoEngineError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|source| GeoEngineError::DatabaseOpen {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(sha256_hex(&bytes)))
}

fn has_nonempty_file(path: &Path) -> Result<bool, GeoEngineError> {
    if !path.exists() {
        return Ok(false);
    }
    let metadata = fs::metadata(path).map_err(|source| GeoEngineError::DatabaseOpen {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(metadata.len() > 0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

fn write_checksum_file(path: &Path, checksum: &str) -> Result<(), GeoEngineError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| GeoEngineError::DatabaseOpen {
        path: parent.to_path_buf(),
        source,
    })?;
    fs::write(path, format!("{checksum}\n")).map_err(|source| GeoEngineError::DatabaseOpen {
        path: path.to_path_buf(),
        source,
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

fn format_place(result: &LookupResult) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(4);
    if let Some(subdistrict) = result.subdistrict.as_ref() {
        parts.push(subdistrict.name.as_str());
    }
    if let Some(district) = result.district.as_ref() {
        parts.push(district.name.as_str());
    }
    if let Some(state) = result.state.as_ref() {
        parts.push(state.name.as_str());
    }
    parts.push(result.country.name.as_str());
    parts.join(", ")
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
    let is_all_caps = name
        .chars()
        .any(|c| c.is_alphabetic())
        && !name.chars().any(|c| c.is_lowercase());
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
