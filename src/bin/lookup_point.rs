use std::env;
use std::path::Path;
use std::process;

const DEFAULT_ASSET_DIR: &str = ".";

fn main() {
    let mut args = env::args().skip(1);
    let lat = parse_coord(args.next(), "latitude");
    let lon = parse_coord(args.next(), "longitude");
    let asset_dir_path = args.next().unwrap_or_else(|| DEFAULT_ASSET_DIR.to_string());
    let asset_dir = Path::new(&asset_dir_path);

    if args.next().is_some() {
        print_usage_and_exit("received too many arguments");
    }

    if let Err(err) = geo_engine::init_path(asset_dir,true) {
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
    eprintln!("Usage: cargo run --bin lookup_point -- <latitude> <longitude> [asset_dir]");
    process::exit(2);
}
