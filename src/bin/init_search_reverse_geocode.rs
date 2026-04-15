use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let (asset_dir, query, lat, lon) = parse_args(env::args().skip(1))?;
    fs::create_dir_all(&asset_dir)?;

    // Use local cache and local manifest when present.
    unsafe {
        env::set_var("GEO_ENGINE_CACHE_DIR", &asset_dir);
        if let Some(manifest_path) = default_manifest_override_path() {
            env::set_var("GEO_ENGINE_RELEASE_MANIFEST_PATH", manifest_path);
        }
    }

    println!("asset dir: {}", asset_dir.display());

    // 1) Init
    let _paths = geo_engine::init_all_assets(&asset_dir, true)?;
    let engine = geo_engine::init_geo_engine_with_config(&asset_dir, true)?;

    // 2) Search
    let search_result = engine.search_places_by_name(&query, Some(5))?;
    println!(
        "search \"{}\" => cities={}, subdistricts={}",
        query,
        search_result.cities.len(),
        search_result.subdistricts.len()
    );

    if let Some(city) = search_result.cities.first() {
        println!(
            "top city: {} ({}) at {}, {}",
            city.name, city.country_code, city.latitude, city.longitude
        );
    }

    if let Some(subdistrict) = search_result.subdistricts.first() {
        println!(
            "top subdistrict: {} ({})",
            subdistrict.subdistrict.name, subdistrict.subdistrict.iso2
        );
    }

    // 3) Reverse geocode
    let reverse = engine.reverse_geocoding(lat, lon)?;
    println!(
        "reverse ({}, {}) => {}, {}",
        lat, lon, reverse.country.name, reverse.city.name
    );

    if let Some(state) = reverse.state {
        println!("state: {} ({})", state.name, state.iso2);
    }
    if let Some(district) = reverse.district {
        println!("district: {} ({})", district.name, district.iso2);
    }
    if let Some(subdistrict) = reverse.subdistrict {
        println!("subdistrict: {} ({})", subdistrict.name, subdistrict.iso2);
    }

    Ok(())
}

fn default_manifest_override_path() -> Option<PathBuf> {
    let manifest_path = Path::new("assets-manifest.json");
    if manifest_path.exists() {
        Some(manifest_path.to_path_buf())
    } else {
        None
    }
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<(PathBuf, String, f32, f32), Box<dyn Error>> {
    let asset_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./release-assets"));
    let query = args.next().unwrap_or_else(|| "bihar".to_string());
    let lat = args
        .next()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(25.5941);
    let lon = args
        .next()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(85.1376);

    if args.next().is_some() {
        return Err(
            "Usage: cargo run --bin init_search_reverse_geocode -- [asset_dir] [query] [lat] [lon]"
                .into(),
        );
    }

    Ok((asset_dir, query, lat, lon))
}
