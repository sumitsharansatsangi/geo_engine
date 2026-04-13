use std::env;
use std::path::Path;
use std::process;

const DEFAULT_CITY_FST: &str = "cities-0.0.1.fst";
const DEFAULT_CITY_RKYV: &str = "cities-0.0.1.rkyv";

fn main() {
    let mut args = env::args().skip(1);
    let lat = parse_coord(args.next(), "latitude");
    let lon = parse_coord(args.next(), "longitude");
    let geo_path = args.next().unwrap_or_else(|| {
        print_usage_and_exit("missing geo.db path");
    });
    let subdistrict_path = args.next().unwrap_or_else(|| {
        print_usage_and_exit("missing subdistrict.db path");
    });

    let geo_db = Path::new(&geo_path);
    let subdistrict_db = Path::new(&subdistrict_path);
    let city_fst_path = args.next().unwrap_or_else(|| DEFAULT_CITY_FST.to_string());
    let city_rkyv_path = args.next().unwrap_or_else(|| DEFAULT_CITY_RKYV.to_string());
    let city_fst = Path::new(&city_fst_path);
    let city_rkyv = Path::new(&city_rkyv_path);

    if args.next().is_some() {
        print_usage_and_exit("received too many arguments");
    }

    if let Err(err) = geo_engine::init_path(geo_db, subdistrict_db, city_fst, city_rkyv) {
        eprintln!("Init failed: {err}");
        process::exit(1);
    }

    match geo_engine::reverse_geocoding(lat, lon) {
        Ok(result) => {
            println!("Latitude: {lat}");
            println!("Longitude: {lon}");
            println!("Country: {} ({})", result.country.name, result.country.iso2);

            if let Some(state) = result.state {
                println!("State: {} ({})", state.name, state.iso2);
            }

            if let Some(district) = result.district {
                println!("District: {} ({})", district.name, district.iso2);
            }

            if let Some(subdistrict) = result.subdistrict {
                println!("Subdistrict: {} ({})", subdistrict.name, subdistrict.iso2);
            }

            println!(
                "Nearest City: {} ({})",
                result.city.name, result.city.country_code
            );
        }
        Err(err) => {
            eprintln!("Reverse geocoding failed: {err}");
            process::exit(1);
        }
    }
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
    eprintln!(
        "Usage: cargo run --bin lookup_point -- <latitude> <longitude> <geo.db path> <subdistrict.db path>"
    );
    eprintln!(
        "       cargo run --bin lookup_point -- <latitude> <longitude> <geo.db path> <subdistrict.db path> [cities.fst path] [cities.rkyv path]"
    );
    process::exit(2);
}
