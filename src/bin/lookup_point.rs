use std::env;
use std::process;

fn main() {
    let mut args = env::args().skip(1);
    let lat = parse_coord(args.next(), "latitude");
    let lon = parse_coord(args.next(), "longitude");

    if args.next().is_some() {
        print_usage_and_exit("received too many arguments");
    }

    match geo_engine::lookup(lat, lon) {
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

            match geo_engine::lookup_place(lat, lon) {
                Ok(place) => println!("Place: {place}"),
                Err(err) => eprintln!("Place format failed: {err}"),
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
    eprintln!("Usage: cargo run --bin lookup_point -- <latitude> <longitude>");
    process::exit(2);
}
