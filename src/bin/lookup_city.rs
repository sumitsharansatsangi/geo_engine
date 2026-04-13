use fst::{IntoStreamer, Map, Streamer};
use rkyv::Archived;
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::Path;
use std::process;

#[path = "../engine/city.rs"]
mod city;
use city::{City, normalize};

const DEFAULT_FST_PATH: &str = "cities.fst";
const DEFAULT_RKYV_PATH: &str = "cities.rkyv";
const DEFAULT_LIMIT: usize = 20;

fn main() {
    let mut args = env::args().skip(1);
    let query = args.next().unwrap_or_else(|| {
        print_usage_and_exit("missing search query");
    });

    let mut limit = DEFAULT_LIMIT;
    let mut fst_path = DEFAULT_FST_PATH.to_string();
    let mut rkyv_path = DEFAULT_RKYV_PATH.to_string();

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
                rkyv_path = extra[2].clone();
            }
            if extra.len() > 3 {
                print_usage_and_exit("received too many arguments");
            }
        } else {
            fst_path = first.clone();
            if extra.len() > 1 {
                rkyv_path = extra[1].clone();
            }
            if extra.len() > 2 {
                print_usage_and_exit("received too many arguments");
            }
        }
    }

    let normalized = normalize(query.trim());
    if normalized.is_empty() {
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

    let cities_by_id = load_cities_by_id(Path::new(&rkyv_path)).unwrap_or_else(|err| {
        eprintln!("failed to load '{}': {err}", rkyv_path);
        process::exit(1);
    });

    let prefix = format!("{}|", normalized);
    let upper = format!("{}\u{10FFFF}", prefix);
    let mut stream = fst
        .range()
        .ge(prefix.as_str())
        .lt(upper.as_str())
        .into_stream();

    let mut matched_ids: BTreeSet<u32> = BTreeSet::new();
    while let Some((_key, value)) = stream.next() {
        matched_ids.insert(value as u32);
        if matched_ids.len() >= limit {
            break;
        }
    }

    if matched_ids.is_empty() {
        println!("No matches found for query: {query}");
        return;
    }

    println!("Matches for '{query}' (normalized: '{normalized}'):");

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

fn load_cities_by_id(path: &Path) -> Result<HashMap<u32, City>, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    let archived: &Archived<Vec<City>> =
        rkyv::access::<Archived<Vec<City>>, rkyv::rancor::Error>(&bytes)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(&bytes) });

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

fn print_usage_and_exit(message: &str) -> ! {
    eprintln!("{message}");
    eprintln!("Usage:");
    eprintln!(
        "  cargo run --bin lookup_city -- <query> [limit] [cities.fst path] [cities.rkyv path]"
    );
    eprintln!("  cargo run --bin lookup_city -- <query> [cities.fst path] [cities.rkyv path]");
    process::exit(2);
}
