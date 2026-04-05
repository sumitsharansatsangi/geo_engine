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
    eprintln!("Usage: cargo run --bin lookup_subdistrict_point -- <latitude> <longitude>");
    process::exit(2);
}
