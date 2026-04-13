use std::env;
use std::path::Path;
use std::process;

fn main() {
    let mut args = env::args().skip(1);
    let first = args.next().unwrap_or_else(|| {
        print_usage_and_exit("missing arguments");
    });

    if is_search_flag(&first) {
        let query = args.next().unwrap_or_else(|| {
            print_usage_and_exit("missing search query");
        });
        let subdistrict_path = args
            .next()
            .unwrap_or_else(|| DEFAULT_SUBDISTRICT_DB.to_string());

        if args.next().is_some() {
            print_usage_and_exit("received too many arguments for search mode");
        }

        run_search(&query, Path::new(&subdistrict_path));
        return;
    }

    let lat = parse_coord(Some(first), "latitude");
    let lon = parse_coord(args.next(), "longitude");
    let geo_path = args.next().unwrap_or_else(|| {
        print_usage_and_exit("missing geo.db path");
    });
    let subdistrict_path = args.next().unwrap_or_else(|| {
        print_usage_and_exit("missing subdistrict.db path");
    });
    let data_path = args.next().unwrap_or_else(|| DEFAULT_DATA_CSV.to_string());

    if args.next().is_some() {
        print_usage_and_exit("received too many arguments");
    }

    run_point_lookup(
        lat,
        lon,
        Path::new(&geo_path),
        Path::new(&subdistrict_path),
        Path::new(&data_path),
    );
}

const DEFAULT_SUBDISTRICT_DB: &str = "subdistrict.db";
const DEFAULT_GEO_DB: &str = "geo.db";
const DEFAULT_CITY_FST: &str = "cities-0.0.1.fst";
const DEFAULT_CITY_RKYV: &str = "cities-0.0.1.rkyv";
const DEFAULT_DATA_CSV: &str = "data.csv";

fn is_search_flag(value: &str) -> bool {
    matches!(value, "--search" | "search" | "-s")
}

fn run_search(query: &str, subdistrict_db: &Path) {
    let geo_db = Path::new(DEFAULT_GEO_DB);
    let city_fst = Path::new(DEFAULT_CITY_FST);
    let city_rkyv = Path::new(DEFAULT_CITY_RKYV);

    if let Err(err) = geo_engine::init_path(geo_db, subdistrict_db, city_fst, city_rkyv) {
        eprintln!("Init failed: {err}");
        process::exit(1);
    }

    match geo_engine::search(query) {
        Ok(results) => {
            if results.subdistricts.is_empty() {
                eprintln!("No subdistrict found for query: {query}");
                process::exit(1);
            }

            for matched in results.subdistricts {
                println!(
                    "{}, {}, {}",
                    matched.subdistrict.name, matched.district.name, matched.state.name
                );
            }
        }
        Err(err) => {
            eprintln!("Search failed: {err}");
            process::exit(1);
        }
    }
}

fn run_point_lookup(lat: f32, lon: f32, geo_db: &Path, subdistrict_db: &Path, data_csv: &Path) {
    match geo_engine::engine::api::lookup_with_subdistrict_path(
        lat,
        lon,
        geo_db,
        Some(subdistrict_db),
    ) {
        Ok(result) => {
            if !result.country.name.eq_ignore_ascii_case("india") {
                eprintln!("Point is outside India");
                process::exit(1);
            }

            let mut parts: Vec<&str> = Vec::new();
            if let Some(subdistrict) = result.subdistrict.as_ref() {
                parts.push(subdistrict.name.as_str());
            }
            if let Some(district) = result.district.as_ref() {
                parts.push(district.name.as_str());
            }
            if let Some(state) = result.state.as_ref() {
                parts.push(state.name.as_str());
            }

            if parts.is_empty() {
                eprintln!("No India administrative match found");
                process::exit(1);
            }

            println!("{}", parts.join(", "));

            let Some(district) = result.district.as_ref() else {
                eprintln!("No district found for demographic lookup");
                process::exit(1);
            };

            if let Some(demographics) = result.demographics.as_ref() {
                print_demographics(&demographics.major_religion, &demographics.languages);
                return;
            }

            let profiles = match geo_engine::district_data::load_district_profiles(data_csv) {
                Ok(profiles) => profiles,
                Err(err) => {
                    eprintln!(
                        "Failed to read demographics CSV '{}': {err}",
                        data_csv.display()
                    );
                    process::exit(1);
                }
            };

            match geo_engine::district_data::find_district_profile(
                &profiles,
                &district.iso2,
                &district.name,
            ) {
                Some(profile) => print_demographics(&profile.major_religion, &profile.languages),
                None => {
                    eprintln!(
                        "No demographics found in '{}' for district {} ({})",
                        data_csv.display(),
                        district.name,
                        district.iso2
                    );
                    process::exit(1);
                }
            }
        }
        Err(err) => {
            eprintln!("Lookup failed: {err}");
            process::exit(1);
        }
    }
}

fn print_demographics(major_religion: &str, languages: &[geo_engine::district_data::GeoLanguage]) {
    println!("Religion: {major_religion}");
    println!(
        "Languages: {}",
        languages
            .iter()
            .map(|language| format!("{} ({})", language.name, language.usage_type))
            .collect::<Vec<String>>()
            .join(", ")
    );
}

fn parse_coord(value: Option<String>, label: &str) -> f32 {
    let raw = value.unwrap_or_else(|| {
        print_usage_and_exit(&format!("missing {label}"));
    });

    raw.parse::<f32>().unwrap_or_else(|_| {
        print_usage_and_exit(&format!("invalid {label}: {raw}"));
    })
}

fn print_usage_and_exit(message: &str) -> ! {
    eprintln!("{message}");
    eprintln!("Usage:");
    eprintln!(
        "  cargo run --bin lookup_subdistrict_point -- <latitude> <longitude> <geo.db path> <subdistrict.db path> [data.csv path]"
    );
    eprintln!(
        "  cargo run --bin lookup_subdistrict_point -- --search <query> [subdistrict.db path]"
    );
    process::exit(2);
}
