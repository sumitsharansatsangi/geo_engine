fn main() {
    let lat = 25.25;
    let lon = 87.04;

    let engine = match geo_engine::engine::bootstrap::init_geo_engine() {
        Ok(engine) => engine,
        Err(err) => {
            eprintln!("Initialization failed: {}", err);
            std::process::exit(1);
        }
    };

    match engine.lookup(lat, lon) {
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
            println!("Center: {}, {}", result.latitude, result.longitude);
        }
        Err(err) => {
            eprintln!("Lookup failed: {}", err);
            std::process::exit(1);
        }
    }
}
