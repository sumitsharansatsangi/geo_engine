use geo_engine::{init_path, search};
fn main() {
    match init_path("assets".to_owned(), true) {
        Ok(status) => {
            if status {
                println!("Initialization successful");
            } else {
                println!("Initialization returned false");
            }
        }

        Err(err) => eprintln!("Initialization failed: {err}"),
    };

    match search("jhajha") {
        Ok(results) => {
            println!("Search successful");
            for city in results.cities {
                println!("City: {}, {}", city.name, city.country_code);
            }
            for subdistrict in results.subdistricts {
                println!(
                    "Subdistrict: {}, {}, {}",
                    subdistrict.subdistrict.name,
                    subdistrict.district.name,
                    subdistrict.state.name
                );
            }
        }

        Err(err) => eprintln!("Search failed: {err}"),

    }

    match geo_engine::reverse_geocoding(25.25, 87.04) {
        Ok(result) => {
            println!("Reverse geocoding successful");
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

        Err(err) => eprintln!("Reverse geocoding failed: {err}"),
    }

}
