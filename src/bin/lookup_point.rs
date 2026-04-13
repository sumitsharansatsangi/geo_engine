use std::env;
use std::path::Path;
use std::process;

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

    if args.next().is_some() {
        print_usage_and_exit("received too many arguments");
    }

    let geo_db = Path::new(&geo_path);
    let subdistrict_db = Path::new(&subdistrict_path);

    match geo_engine::engine::api::lookup_with_subdistrict_path(
        lat,
        lon,
        geo_db,
        Some(subdistrict_db),
    ) {
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
        }
        Err(err) => {
            eprintln!("Lookup failed: {err}");
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
    process::exit(2);
}
