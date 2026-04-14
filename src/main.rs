fn main() {
    let lat = 25.25;
    let lon = 87.04;

    let asset_dir = std::path::Path::new(".");

    if let Err(err) = geo_engine::init_path(asset_dir) {
        eprintln!("Initialization failed: {}", err);
        std::process::exit(1);
    }

    match geo_engine::reverse_geocoding(lat, lon) {
        Ok(result) => {
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
            eprintln!("Reverse geocoding failed: {}", err);
            std::process::exit(1);
        }
    }
}
