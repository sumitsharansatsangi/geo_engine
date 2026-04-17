use fst::{IntoStreamer, Map, Streamer};
use rkyv::Archived;
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::Path;
use std::process;

#[path = "../engine/city.rs"]
mod city;
use city::{City, CityCore, CityMeta, normalize_keys};

const DEFAULT_FST_PATH: &str = "cities.fst";
const DEFAULT_CORE_PATH: &str = "cities.core";
const DEFAULT_META_PATH: &str = "cities.meta";
const DEFAULT_LIMIT: usize = 20;

fn main() {
    let mut args = env::args().skip(1);
    let query = args.next().unwrap_or_else(|| {
        print_usage_and_exit("missing search query");
    });

    let mut limit = DEFAULT_LIMIT;
    let mut fst_path = DEFAULT_FST_PATH.to_string();
    let mut core_path = DEFAULT_CORE_PATH.to_string();
    let mut meta_path = DEFAULT_META_PATH.to_string();

    let mut extra = Vec::new();
    for arg in args {
        extra.push(arg);
    }

    if let Some(first) = extra.first() {
        if let Ok(parsed_limit) = first.parse::<usize>() {
            limit = parsed_limit.max(1);
            if extra.len() > 1 {
                fst_path = extra[1].clone();
            }
            if extra.len() > 2 {
                core_path = extra[2].clone();
            }
            if extra.len() > 3 {
                meta_path = extra[3].clone();
            }
            if extra.len() > 4 {
                print_usage_and_exit("received too many arguments");
            }
        } else {
            fst_path = first.clone();
            if extra.len() > 1 {
                core_path = extra[1].clone();
            }
            if extra.len() > 2 {
                meta_path = extra[2].clone();
            }
            if extra.len() > 3 {
                print_usage_and_exit("received too many arguments");
            }
        }
    }

    let normalized_keys = normalize_keys(query.trim());
    if normalized_keys.is_empty() {
        print_usage_and_exit("query cannot be empty");
    }

    let fst_bytes = fs::read(Path::new(&fst_path)).unwrap_or_else(|err| {
        eprintln!("failed to open '{}': {err}", fst_path);
        process::exit(1);
    });
    let fst = Map::new(fst_bytes).unwrap_or_else(|err| {
        eprintln!("failed to parse '{}': {err}", fst_path);
        process::exit(1);
    });

    let cities_by_id = load_cities_by_id(Path::new(&core_path), Path::new(&meta_path))
        .unwrap_or_else(|err| {
            eprintln!(
                "failed to load city assets '{}', '{}': {err}",
                core_path, meta_path
            );
            process::exit(1);
        });

    let mut matched_ids: BTreeSet<u32> = BTreeSet::new();
    for normalized in &normalized_keys {
        let prefix = format!("{}|", normalized);
        let upper = format!("{}\u{10FFFF}", prefix);
        let mut stream = fst
            .range()
            .ge(prefix.as_str())
            .lt(upper.as_str())
            .into_stream();

        while let Some((_key, value)) = stream.next() {
            matched_ids.insert(value as u32);
            if matched_ids.len() >= limit {
                break;
            }
        }

        if matched_ids.len() >= limit {
            break;
        }
    }

    if matched_ids.is_empty() {
        println!("No matches found for query: {query}");
        return;
    }

    println!(
        "Matches for '{query}' (normalized keys: {:?}):",
        normalized_keys
    );

    for geoname_id in matched_ids {
        if let Some(city) = cities_by_id.get(&geoname_id) {
            let admin1 = city.admin1_name.as_deref().unwrap_or("");
            let admin1_code = city.admin1_code.as_deref().unwrap_or("");
            let admin2 = city.admin2_name.as_deref().unwrap_or("");
            let admin2_code = city.admin2_code.as_deref().unwrap_or("");

            println!(
                "- {} ({}) | {} | admin1: {} [{}] | admin2: {} [{}] | lat: {:.6}, lon: {:.6} | geoname_id: {}",
                city.name,
                city.ascii,
                city.country_code,
                admin1,
                admin1_code,
                admin2,
                admin2_code,
                city.lat,
                city.lon,
                city.geoname_id,
            );
        }
    }
}

fn load_cities_by_id(core_path: &Path, meta_path: &Path) -> Result<HashMap<u32, City>, String> {
    let core_bytes = fs::read(core_path).map_err(|err| err.to_string())?;
    let meta_bytes = fs::read(meta_path).map_err(|err| err.to_string())?;

    let archived_core: &Archived<Vec<CityCore>> =
        rkyv::access::<Archived<Vec<CityCore>>, rkyv::rancor::Error>(&core_bytes)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(&core_bytes) });
    let archived_meta: &Archived<CityMeta> =
        rkyv::access::<Archived<CityMeta>, rkyv::rancor::Error>(&meta_bytes)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(&meta_bytes) });

    let mut cities = HashMap::with_capacity(archived_core.len());
    for archived_city in archived_core.iter() {
        let city = City {
            geoname_id: archived_city.geoname_id.into(),
            country_code: resolve_string(
                archived_meta,
                u32::from(archived_city.country_code_id) as usize,
            )?,
            name: resolve_string(archived_meta, u32::from(archived_city.name_id) as usize)?,
            ascii: resolve_string(archived_meta, u32::from(archived_city.ascii_id) as usize)?,
            admin1_code: resolve_optional_string(
                archived_meta,
                archived_city
                    .admin1_code_id
                    .as_ref()
                    .map(|value| u32::from(*value)),
            )?,
            admin1_name: resolve_optional_string(
                archived_meta,
                archived_city
                    .admin1_name_id
                    .as_ref()
                    .map(|value| u32::from(*value)),
            )?,
            admin2_code: resolve_optional_string(
                archived_meta,
                archived_city
                    .admin2_code_id
                    .as_ref()
                    .map(|value| u32::from(*value)),
            )?,
            admin2_name: resolve_optional_string(
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

fn print_usage_and_exit(message: &str) -> ! {
    eprintln!("{message}");
    eprintln!("Usage:");
    eprintln!(
        "  cargo run --bin lookup_city -- <query> [limit] [cities.fst path] [cities.core path] [cities.meta path]"
    );
    eprintln!(
        "  cargo run --bin lookup_city -- <query> [cities.fst path] [cities.core path] [cities.meta path]"
    );
    process::exit(2);
}

fn resolve_string(meta: &Archived<CityMeta>, index: usize) -> Result<String, String> {
    meta.strings
        .get(index)
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| format!("invalid city string index: {index}"))
}

fn resolve_optional_string(
    meta: &Archived<CityMeta>,
    value: Option<u32>,
) -> Result<Option<String>, String> {
    match value {
        Some(index) => resolve_string(meta, index as usize).map(Some),
        None => Ok(None),
    }
}
