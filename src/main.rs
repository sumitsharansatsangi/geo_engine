fn main() {
    let lat = 25.25;
    let lon = 87.04;

    match geo_engine::lookup_with_subdistrict_path(
        lat,
        lon,
        std::path::Path::new("geo.db"),
        Some(std::path::Path::new("subdistrict.db")),
    ) {
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
