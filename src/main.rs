fn main() {
    let lat = 25.25;
    let lon = 87.04;

    match geo_engine::lookup(lat, lon) {
        Ok(result) => {
            println!("Country: {} ({})", result.country.name, result.country.iso2);
            if let Some(state) = result.state {
                println!("State: {} ({})", state.name, state.iso2);
            }
            if let Some(district) = result.district {
                println!("District: {} ({})", district.name, district.iso2);
            }
        }
        Err(err) => {
            eprintln!("Lookup failed: {}", err);
            std::process::exit(1);
        }
    }
}
