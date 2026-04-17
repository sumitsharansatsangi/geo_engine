use fst::MapBuilder;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zip::ZipArchive;

#[path = "../engine/city.rs"]
mod city;
use city::{CityCore, CityMeta, normalize_keys};

// ── CityPoint: only used for building city index files ──
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone, Copy)]
struct CityPoint {
    id: u32,
    lat: f32,
    lon: f32,
}

// ----------- MAIN -----------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_args(env::args().skip(1))?;
    let version = config.version;
    let fst_path = versioned_name(&version, "fst");
    let core_path = versioned_name(&version, "core");
    let meta_path = versioned_name(&version, "meta");
    let points_path = versioned_name(&version, "points");
    let geo_db_path = PathBuf::from(format!("release-assets/geo-{version}.db"));

    // ---- FST ----
    if let Some(parent) = fst_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let fst_file = File::create(&fst_path)?;
    let mut fst = MapBuilder::new(fst_file)?;
    let mut city_keys: Vec<(String, u64)> = Vec::new();

    let admin1_lookup = load_admin1_lookup()?;
    let admin2_lookup = load_admin2_lookup()?;
    let country_names = load_country_names(&geo_db_path)?;
    let city_enrichment_index =
        load_city_enrichment_index(&config.cities_dir, config.worldcities_csv_path.as_deref())?;

    let lines = load_cities500_lines(config.cities500_path.as_deref())?;

    let mut string_pool = StringPool::new();
    let mut cities: Vec<CityCore> = Vec::new();
    let mut points: Vec<CityPoint> = Vec::new();

    // ---- PARSE ----
    for line in lines {
        let mut p = line.split('\t');

        let geoname_id: u32 = p.next().unwrap_or("0").parse().unwrap_or(0);
        let name = p.next().unwrap_or("");
        let ascii = p.next().unwrap_or("");
        let alt = p.next().unwrap_or("");
        let lat: f32 = p.next().unwrap_or("0").parse().unwrap_or(0.0);
        let lon: f32 = p.next().unwrap_or("0").parse().unwrap_or(0.0);
        let _feature_class = p.next().unwrap_or("");
        let _feature_code = p.next().unwrap_or("");
        let country_code = p.next().unwrap_or("");
        let _cc2 = p.next().unwrap_or("");
        let admin1_code = normalize_optional(p.next().unwrap_or(""));
        let admin2_code = normalize_optional(p.next().unwrap_or(""));

        let admin1_name = admin1_code.as_ref().and_then(|code| {
            admin1_lookup
                .get(&admin1_lookup_key(country_code, code))
                .cloned()
        });
        let admin2_name = match (&admin1_code, &admin2_code) {
            (Some(admin1_code), Some(admin2_code)) => admin2_lookup
                .get(&admin2_lookup_key(country_code, admin1_code, admin2_code))
                .cloned(),
            _ => None,
        };

        // ---- STORE POINT ----
        points.push(CityPoint {
            id: geoname_id,
            lat,
            lon,
        });

        // ---- FST ----
        for key in city_key_variants(
            country_code,
            admin1_code.as_deref(),
            admin2_code.as_deref(),
            geoname_id,
            name,
        ) {
            collect_key(&mut city_keys, key, geoname_id as u64);
        }
        for key in city_key_variants(
            country_code,
            admin1_code.as_deref(),
            admin2_code.as_deref(),
            geoname_id,
            ascii,
        ) {
            collect_key(&mut city_keys, key, geoname_id as u64);
        }

        if let Some(country_name) = country_names.get(country_code) {
            add_city_enrichment_keys(
                &mut city_keys,
                &city_enrichment_index,
                country_name,
                country_code,
                admin1_code.as_deref(),
                admin2_code.as_deref(),
                geoname_id,
                name,
                ascii,
                lat,
                lon,
            );
        }

        for a in alt.split(',').filter(|s| !s.is_empty()) {
            if !a.is_empty() {
                for key in city_key_variants(
                    country_code,
                    admin1_code.as_deref(),
                    admin2_code.as_deref(),
                    geoname_id,
                    a,
                ) {
                    collect_key(&mut city_keys, key, geoname_id as u64);
                }
            }
        }

        // ---- STORE CITY CORE ----
        cities.push(CityCore {
            geoname_id,
            country_code_id: string_pool.intern(country_code),
            name_id: string_pool.intern(name),
            ascii_id: string_pool.intern(ascii),
            admin1_code_id: admin1_code
                .as_deref()
                .map(|value| string_pool.intern(value)),
            admin1_name_id: admin1_name
                .as_deref()
                .map(|value| string_pool.intern(value)),
            admin2_code_id: admin2_code
                .as_deref()
                .map(|value| string_pool.intern(value)),
            admin2_name_id: admin2_name
                .as_deref()
                .map(|value| string_pool.intern(value)),
            lat,
            lon,
        });
    }

    city_keys
        .sort_unstable_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    city_keys.dedup_by(|left, right| left.0 == right.0);

    for (key, value) in city_keys {
        fst.insert(key, value)?;
    }

    fst.finish()?;

    // ---- SAVE RKYV (Cities Core + Meta) ----
    let city_core_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cities)?;
    std::fs::write(&core_path, &city_core_bytes)?;

    let city_meta = CityMeta {
        strings: string_pool.into_vec(),
    };
    let city_meta_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&city_meta)?;
    std::fs::write(&meta_path, &city_meta_bytes)?;

    // ---- SAVE POINTS (NOT RTREE!) ----
    let point_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&points)?;
    std::fs::write(&points_path, &point_bytes)?;

    println!("✅ Build complete:");
    println!("  - {}", fst_path.display());
    println!("  - {}", core_path.display());
    println!("  - {}", meta_path.display());
    println!("  - {}", points_path.display());

    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<BuildCityConfig, Box<dyn std::error::Error>> {
    let mut version = String::from("0.0.1");
    let mut cities500_path: Option<PathBuf> = None;
    let mut worldcities_csv_path: Option<PathBuf> = None;
    let mut cities_dir = PathBuf::from("cities");

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => {
                let value = args.next().ok_or("missing value for --version")?;
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err("--version cannot be empty".into());
                }
                version = trimmed.to_string();
            }
            "--cities500" => {
                let value = args.next().ok_or("missing value for --cities500")?;
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err("--cities500 cannot be empty".into());
                }
                cities500_path = Some(PathBuf::from(trimmed));
            }
            "--worldcities-csv" => {
                let value = args.next().ok_or("missing value for --worldcities-csv")?;
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err("--worldcities-csv cannot be empty".into());
                }
                worldcities_csv_path = Some(PathBuf::from(trimmed));
            }
            "--cities-dir" => {
                let value = args.next().ok_or("missing value for --cities-dir")?;
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err("--cities-dir cannot be empty".into());
                }
                cities_dir = PathBuf::from(trimmed);
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }

    Ok(BuildCityConfig {
        version,
        cities500_path,
        worldcities_csv_path,
        cities_dir,
    })
}

fn versioned_name(version: &str, ext: &str) -> PathBuf {
    PathBuf::from(format!("release-assets/cities-{version}.{ext}"))
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!(
        "  cargo run --bin build_city -- [--version X.Y.Z] [--cities500 /path/to/cities500.txt] [--worldcities-csv /path/to/worldcities.csv] [--cities-dir /path/to/cities]"
    );
}

fn load_cities500_lines(
    cities500_path: Option<&Path>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if let Some(path) = cities500_path {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        return Ok(reader.lines().collect::<Result<Vec<_>, _>>()?);
    }

    let client = http_client()?;
    let bytes = client
        .get("https://download.geonames.org/export/dump/cities500.zip")
        .send()?
        .error_for_status()?
        .bytes()?;

    let reader = Cursor::new(bytes);
    let mut zip = ZipArchive::new(reader)?;
    let file = zip.by_name("cities500.txt")?;
    let reader = BufReader::new(file);
    Ok(reader.lines().collect::<Result<Vec<_>, _>>()?)
}

fn add_city_enrichment_keys(
    city_keys: &mut Vec<(String, u64)>,
    enrichment_index: &HashMap<(String, String), Vec<CityEnrichment>>,
    country_name: &str,
    country_code: &str,
    admin1_code: Option<&str>,
    admin2_code: Option<&str>,
    geoname_id: u32,
    name: &str,
    ascii: &str,
    lat: f32,
    lon: f32,
) {
    let country_keys = normalize_keys(country_name);
    if country_keys.is_empty() {
        return;
    }

    for lookup_name in [name, ascii] {
        let name_keys = normalize_keys(lookup_name);
        let mut candidates = None;
        'lookup: for country_key in &country_keys {
            for name_key in &name_keys {
                if let Some(matches) =
                    enrichment_index.get(&(country_key.clone(), name_key.clone()))
                {
                    candidates = Some(matches);
                    break 'lookup;
                }
            }
        }

        let Some(candidates) = candidates else {
            continue;
        };

        let Some(best_match) = candidates.iter().min_by(|left, right| {
            let left_distance = haversine_km(lat, lon, left.latitude, left.longitude);
            let right_distance = haversine_km(lat, lon, right.latitude, right.longitude);
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) else {
            continue;
        };

        for alias in &best_match.aliases {
            for key in city_key_variants(country_code, admin1_code, admin2_code, geoname_id, alias)
            {
                collect_key(city_keys, key, geoname_id as u64);
            }
        }

        break;
    }
}

fn load_country_names(
    _geo_db_path: &Path,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let client = http_client()?;
    let bytes = client
        .get("https://raw.githubusercontent.com/datasets/geo-countries/main/data/countries.geojson")
        .send()?
        .error_for_status()?
        .bytes()?;
    let root: Value = serde_json::from_slice(&bytes)?;
    let features = root
        .get("features")
        .and_then(Value::as_array)
        .ok_or("invalid countries.geojson: missing features")?;

    let mut names = HashMap::with_capacity(features.len());
    for feature in features {
        let Some(properties) = feature.get("properties") else {
            continue;
        };

        let country_name = country_name_from_properties(properties);
        let iso2 = country_iso2_from_properties(properties, &country_name);
        let code = String::from_utf8_lossy(&iso2).into_owned();
        names.entry(code).or_insert(country_name);
    }

    Ok(names)
}

fn country_name_from_properties(properties: &Value) -> String {
    properties
        .get("admin")
        .and_then(Value::as_str)
        .or_else(|| properties.get("name").and_then(Value::as_str))
        .or_else(|| properties.get("name_lower").and_then(Value::as_str))
        .unwrap_or("UNKNOWN")
        .trim()
        .to_string()
}

fn country_iso2_from_properties(properties: &Value, country_name: &str) -> [u8; 2] {
    let candidates = ["iso_a2", "iso_a2_eh", "wb_a2", "iso2"];

    for candidate in candidates {
        if let Some(value) = properties.get(candidate).and_then(Value::as_str)
            && let Some(code) = parse_iso2(value)
        {
            return code;
        }
    }

    derive_iso2_from_name(country_name)
}

fn parse_iso2(value: &str) -> Option<[u8; 2]> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-99" {
        return None;
    }

    let mut chars = trimmed
        .bytes()
        .filter(|byte| byte.is_ascii_alphabetic())
        .map(|byte| byte.to_ascii_uppercase());

    let first = chars.next()?;
    let second = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    Some([first, second])
}

fn derive_iso2_from_name(name: &str) -> [u8; 2] {
    let mut code = [b' '; 2];
    let mut chars = name
        .bytes()
        .filter(|byte| byte.is_ascii_alphabetic())
        .map(|byte| byte.to_ascii_uppercase());

    if let Some(first) = chars.next() {
        code[0] = first;
    }
    if let Some(second) = chars.next() {
        code[1] = second;
    }
    code
}

fn load_city_enrichment_index(
    cities_dir: &Path,
    worldcities_csv_path: Option<&Path>,
) -> Result<HashMap<(String, String), Vec<CityEnrichment>>, Box<dyn std::error::Error>> {
    let mut index: HashMap<(String, String), Vec<CityEnrichment>> = HashMap::new();

    if !cities_dir.exists() {
        return Ok(index);
    }

    for entry in fs::read_dir(cities_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let bytes = fs::read(&path)?;
        let records: Vec<CityEnrichmentRecord> = serde_json::from_slice(&bytes)?;

        for record in records {
            let country_keys = normalize_keys(&record.country);
            let name_keys = normalize_keys(&record.name);
            if country_keys.is_empty() || name_keys.is_empty() {
                continue;
            }

            let mut aliases = Vec::new();
            let mut seen = HashSet::new();

            for alias in std::iter::once(record.name)
                .chain(record.other_names.into_values())
                .filter(|value| !value.trim().is_empty())
            {
                let alias_keys = normalize_keys(&alias);
                if alias_keys.is_empty() {
                    continue;
                }

                if alias_keys.iter().all(|key| !seen.insert(key.clone())) {
                    continue;
                }
                aliases.push(alias);
            }

            if aliases.is_empty() {
                continue;
            }

            let enrichment = CityEnrichment {
                latitude: record.latitude,
                longitude: record.longitude,
                aliases,
            };

            for country_key in &country_keys {
                for name_key in &name_keys {
                    index
                        .entry((country_key.clone(), name_key.clone()))
                        .or_default()
                        .push(enrichment.clone());
                }
            }
        }
    }

    if let Some(worldcities_csv_path) = worldcities_csv_path {
        load_worldcities_csv_enrichment(worldcities_csv_path, &mut index)?;
    }

    Ok(index)
}

fn load_worldcities_csv_enrichment(
    worldcities_csv_path: &Path,
    index: &mut HashMap<(String, String), Vec<CityEnrichment>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(worldcities_csv_path)?;

    for row in reader.deserialize::<WorldCitiesRecord>() {
        let row = row?;

        let country_keys = normalize_keys(&row.country);
        let name_keys = normalize_keys(&row.city);
        if country_keys.is_empty() || name_keys.is_empty() {
            continue;
        }

        let mut aliases = Vec::new();
        let mut seen = HashSet::new();

        for alias in [row.city, row.city_ascii]
            .into_iter()
            .filter(|value| !value.trim().is_empty())
        {
            let alias_keys = normalize_keys(&alias);
            if alias_keys.is_empty() {
                continue;
            }

            if alias_keys.iter().all(|key| !seen.insert(key.clone())) {
                continue;
            }
            aliases.push(alias);
        }

        if aliases.is_empty() {
            continue;
        }

        let enrichment = CityEnrichment {
            latitude: row.lat,
            longitude: row.lng,
            aliases,
        };

        for country_key in &country_keys {
            for name_key in &name_keys {
                index
                    .entry((country_key.clone(), name_key.clone()))
                    .or_default()
                    .push(enrichment.clone());
            }
        }
    }

    Ok(())
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

fn collect_key(city_keys: &mut Vec<(String, u64)>, key: String, value: u64) {
    if key.is_empty() {
        return;
    }

    city_keys.push((key, value));
}

fn city_key(
    country_code: &str,
    admin1_code: Option<&str>,
    admin2_code: Option<&str>,
    geoname_id: u32,
    normalized_name: &str,
) -> String {
    if normalized_name.is_empty() {
        return String::new();
    }

    format!(
        "{}|{}|{}|{}|{}",
        normalized_name,
        country_code,
        admin1_code.unwrap_or(""),
        admin2_code.unwrap_or(""),
        geoname_id
    )
}

fn city_key_variants(
    country_code: &str,
    admin1_code: Option<&str>,
    admin2_code: Option<&str>,
    geoname_id: u32,
    raw_name: &str,
) -> Vec<String> {
    normalize_keys(raw_name)
        .into_iter()
        .map(|normalized_name| {
            city_key(
                country_code,
                admin1_code,
                admin2_code,
                geoname_id,
                &normalized_name,
            )
        })
        .filter(|key| !key.is_empty())
        .collect()
}

fn normalize_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn load_admin1_lookup() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let client = http_client()?;
    let bytes = client
        .get("https://download.geonames.org/export/dump/admin1CodesASCII.txt")
        .send()?
        .error_for_status()?
        .bytes()?;
    let reader = BufReader::new(Cursor::new(bytes));
    let mut lookup = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let mut parts = line.split('\t');
        let code = parts.next().unwrap_or("").trim();
        let name = parts.next().unwrap_or("").trim();
        if !code.is_empty() && !name.is_empty() {
            lookup.insert(code.to_string(), name.to_string());
        }
    }

    Ok(lookup)
}

fn load_admin2_lookup() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let client = http_client()?;
    let bytes = client
        .get("https://download.geonames.org/export/dump/admin2Codes.txt")
        .send()?
        .error_for_status()?
        .bytes()?;
    let reader = BufReader::new(Cursor::new(bytes));
    let mut lookup = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let mut parts = line.split('\t');
        let code = parts.next().unwrap_or("").trim();
        let name = parts.next().unwrap_or("").trim();
        if !code.is_empty() && !name.is_empty() {
            lookup.insert(code.to_string(), name.to_string());
        }
    }

    Ok(lookup)
}

fn admin1_lookup_key(country_code: &str, admin1_code: &str) -> String {
    format!("{}.{}", country_code, admin1_code)
}

fn admin2_lookup_key(country_code: &str, admin1_code: &str, admin2_code: &str) -> String {
    format!("{}.{}.{}", country_code, admin1_code, admin2_code)
}

fn http_client() -> Result<reqwest::blocking::Client, Box<dyn std::error::Error>> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30 * 60))
        .build()?)
}

#[derive(Debug, Deserialize)]
struct CityEnrichmentRecord {
    name: String,
    country: String,
    latitude: f32,
    longitude: f32,
    #[serde(default)]
    other_names: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct CityEnrichment {
    latitude: f32,
    longitude: f32,
    aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct WorldCitiesRecord {
    city: String,
    #[serde(default)]
    city_ascii: String,
    lat: f32,
    lng: f32,
    country: String,
}

struct BuildCityConfig {
    version: String,
    cities500_path: Option<PathBuf>,
    worldcities_csv_path: Option<PathBuf>,
    cities_dir: PathBuf,
}

struct StringPool {
    index_by_value: HashMap<String, u32>,
    values: Vec<String>,
}

impl StringPool {
    fn new() -> Self {
        Self {
            index_by_value: HashMap::new(),
            values: Vec::new(),
        }
    }

    fn intern(&mut self, value: &str) -> u32 {
        if let Some(id) = self.index_by_value.get(value) {
            return *id;
        }

        let id = self.values.len() as u32;
        let owned = value.to_string();
        self.values.push(owned.clone());
        self.index_by_value.insert(owned, id);
        id
    }

    fn into_vec(self) -> Vec<String> {
        self.values
    }
}
