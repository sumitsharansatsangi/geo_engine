use std::env;
use std::path::Path;
use std::process;

fn main() {
    let mut args = env::args().skip(1);

    let query = args.next().unwrap_or_else(|| "bihar".to_string());
    let lat = args
        .next()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(25.5941);
    let lon = args
        .next()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(85.1376);
    let asset_dir = args
        .next()
        .map(|value| Path::new(&value).to_path_buf())
        .unwrap_or_else(|| Path::new(".").to_path_buf());

    if args.next().is_some() {
        print_usage_and_exit();
    }

    println!("initializing engine...");
    if let Err(err) = geo_engine::init_path(&asset_dir,true) {
        eprintln!("init failed: {err}");
        process::exit(1);
    }
    println!("engine initialized\n");

    println!("search query: {query}");
    match geo_engine::search(&query) {
        Ok(results) => {
            if results.cities.is_empty() && results.subdistricts.is_empty() {
                println!("  no matches found");
            } else {
                for city in results.cities {
                    print_city(&city, None);
                }

                for subdistrict in results.subdistricts {
                    println!(
                        "  subdistrict: {}, {}, {}",
                        subdistrict.subdistrict.name,
                        subdistrict.district.name,
                        subdistrict.state.name
                    );
                }
            }
        }
        Err(err) => {
            eprintln!("search failed: {err}");
            process::exit(1);
        }
    }

    println!();
    println!("reverse lookup: {lat}, {lon}");
    match geo_engine::reverse_geocoding(lat, lon) {
        Ok(result) => {
            println!(
                "  country: {} ({})",
                result.country.name, result.country.iso2
            );
            if let Some(state) = result.state {
                println!("  state: {} ({})", state.name, state.iso2);
            }
            if let Some(district) = result.district {
                println!("  district: {} ({})", district.name, district.iso2);
            }
            if let Some(subdistrict) = result.subdistrict {
                println!("  subdistrict: {} ({})", subdistrict.name, subdistrict.iso2);
            }
            print_city(&result.city, Some(&result.country.name));
        }
        Err(err) => {
            eprintln!("reverse lookup failed: {err}");
            process::exit(1);
        }
    }
}

fn print_usage_and_exit() -> ! {
    eprintln!("Usage:");
    eprintln!("  cargo run --bin showcase -- [query] [lat] [lon] [asset_dir]");
    process::exit(2);
}

fn print_city(city: &geo_engine::CityMatch, country_name: Option<&str>) {
    println!(
        "  city: {} ({}, {})",
        city.name, city.latitude, city.longitude
    );

    let country_display = country_name.unwrap_or(&city.country_name);
    println!("    country: {}", country_display);
    println!("    country_code: {}", city.country_code);

    if let Some(admin1_name) = city.admin1_name.as_deref() {
        println!(
            "    admin1: {} ({})",
            admin1_name,
            city.admin1_code.as_deref().unwrap_or("")
        );
    }

    if let Some(admin2_name) = city.admin2_name.as_deref() {
        println!(
            "    admin2: {} ({})",
            admin2_name,
            city.admin2_code.as_deref().unwrap_or("")
        );
    }
}
