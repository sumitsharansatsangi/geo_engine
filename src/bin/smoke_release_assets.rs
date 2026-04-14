use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let (asset_dir, query, lat, lon) = parse_args(env::args().skip(1))?;
    fs::create_dir_all(&asset_dir)?;

    // Route the default-cache wrappers to the provided asset folder.
    unsafe {
        env::set_var("GEO_ENGINE_CACHE_DIR", &asset_dir);
    }

    let config = geo_engine::InitConfig {
        asset_dir: asset_dir.clone(),
        verify_checksum: true,
    };

    println!("asset dir: {}", asset_dir.display());

    println!("checking init_all_assets_with_config...");
    let all_paths = geo_engine::init_all_assets_with_config(&config)?;
    print_all_paths("init_all_assets_with_config", &all_paths);

    println!("checking init_city_assets_with_config...");
    let city_paths = geo_engine::init_city_assets_with_config(&config)?;
    print_city_paths("init_city_assets_with_config", &city_paths);

    println!("checking init_geo_engine_with_config...");
    let engine = geo_engine::init_geo_engine_with_config(&config)?;

    println!("checking wrapper init_all_assets...");
    let wrapper_all = geo_engine::init_all_assets(&asset_dir)?;
    print_all_paths("init_all_assets", &wrapper_all);

    println!("checking wrapper init_city_assets...");
    let wrapper_city = geo_engine::init_city_assets()?;
    print_city_paths("init_city_assets", &wrapper_city);

    println!("checking wrapper init_geo_engine...");
    let _wrapper_engine = geo_engine::init_geo_engine()?;

    println!("checking init_path...");
    geo_engine::init_path(&asset_dir)?;

    println!("checking reverse_geocoding...");
    let reverse = geo_engine::reverse_geocoding(lat, lon)?;
    print_reverse("reverse_geocoding", &reverse);

    println!("checking reverse_geocoding_batch...");
    let reverse_batch =
        geo_engine::reverse_geocoding_batch(&[(lat, lon), (lat + 0.01, lon + 0.01)])?;
    println!("  reverse_geocoding_batch size: {}", reverse_batch.len());

    println!("checking search...");
    let search_result = geo_engine::search(&query)?;
    print_search("search", &search_result);

    println!("checking search_batch...");
    let search_batch = geo_engine::search_batch(&[query.clone(), format!("{query} district")])?;
    println!("  search_batch size: {}", search_batch.len());

    println!("checking InitializedGeoEngine::open...");
    let opened = geo_engine::InitializedGeoEngine::open(
        &all_paths.geo_db_path,
        &all_paths.subdistrict_db_path,
        &all_paths.city_fst_path,
        &all_paths.city_rkyv_path,
    )?;

    println!("checking InitializedGeoEngine::lookup...");
    let lookup = opened.lookup(lat, lon)?;
    println!(
        "  lookup country: {} ({})",
        lookup.country.name, lookup.country.iso2
    );

    println!("checking InitializedGeoEngine::reverse_geocoding...");
    let opened_reverse = opened.reverse_geocoding(lat, lon)?;
    print_reverse("InitializedGeoEngine::reverse_geocoding", &opened_reverse);

    println!("checking InitializedGeoEngine::search_places_by_name...");
    let opened_search = opened.search_places_by_name(&query, Some(5))?;
    print_search(
        "InitializedGeoEngine::search_places_by_name",
        &opened_search,
    );

    println!("checking InitializedGeoEngine::open_from_bytes...");
    let country_bytes = fs::read(&all_paths.geo_db_path)?;
    let subdistrict_bytes = fs::read(&all_paths.subdistrict_db_path)?;
    let city_fst_bytes = fs::read(&all_paths.city_fst_path)?;
    let city_rkyv_bytes = fs::read(&all_paths.city_rkyv_path)?;
    let opened_from_bytes = geo_engine::InitializedGeoEngine::open_from_bytes(
        &country_bytes,
        Some(&subdistrict_bytes),
        Some(&city_fst_bytes),
        Some(&city_rkyv_bytes),
    )?;
    let bytes_lookup = opened_from_bytes.lookup(lat, lon)?;
    println!(
        "  open_from_bytes lookup country: {} ({})",
        bytes_lookup.country.name, bytes_lookup.country.iso2
    );

    println!("checking init_all_assets_in_background_with_config...");
    let handle = geo_engine::init_all_assets_in_background_with_config(&config)?;
    let background_paths = handle
        .join()
        .map_err(|_| "init_all_assets_in_background thread panicked")??;
    print_all_paths(
        "init_all_assets_in_background_with_config",
        &background_paths,
    );

    println!("checking init_all_assets_in_background...");
    let handle = geo_engine::init_all_assets_in_background(&asset_dir)?;
    let background_paths = handle
        .join()
        .map_err(|_| "init_all_assets_in_background thread panicked")??;
    print_all_paths("init_all_assets_in_background", &background_paths);

    println!("checking refresh_all_assets_in_background_with_config...");
    let (tx, rx) = mpsc::channel();
    geo_engine::refresh_all_assets_in_background_with_callback_config(&config, move |result| {
        let _ = tx.send(result);
    })?;
    let refresh_result = rx.recv()?;
    let refresh_paths = refresh_result?;
    print_all_paths(
        "refresh_all_assets_in_background_with_callback_config",
        &refresh_paths,
    );

    println!("checking refresh_all_assets_in_background...");
    let (tx, rx) = mpsc::channel();
    geo_engine::refresh_all_assets_in_background_with_callback(&asset_dir, move |result| {
        let _ = tx.send(result);
    })?;
    let refresh_result = rx.recv()?;
    let refresh_paths = refresh_result?;
    print_all_paths(
        "refresh_all_assets_in_background_with_callback",
        &refresh_paths,
    );

    println!("checking refresh_all_assets_in_background_with_config...");
    geo_engine::refresh_all_assets_in_background_with_config(&config)?;

    println!("checking refresh_and_reopen_engine_in_background_with_config...");
    let (tx, rx) = mpsc::channel();
    geo_engine::refresh_and_reopen_engine_in_background_with_config(&config, move |result| {
        let _ = tx.send(result);
    })?;
    let reopened_engine = rx.recv()?;
    let reopened_engine = reopened_engine?;
    let reopened_lookup = reopened_engine.lookup(lat, lon)?;
    println!(
        "  reopened engine country: {} ({})",
        reopened_lookup.country.name, reopened_lookup.country.iso2
    );

    println!("checking refresh_and_reopen_engine_in_background...");
    let (tx, rx) = mpsc::channel();
    geo_engine::refresh_and_reopen_engine_in_background(&asset_dir, move |result| {
        let _ = tx.send(result);
    })?;
    let reopened_engine = rx.recv()?;
    let reopened_engine = reopened_engine?;
    let reopened_lookup = reopened_engine.lookup(lat, lon)?;
    println!(
        "  reopened wrapper country: {} ({})",
        reopened_lookup.country.name, reopened_lookup.country.iso2
    );

    println!("checking refresh_all_assets_in_background...");
    geo_engine::refresh_all_assets_in_background(&asset_dir)?;
    std::thread::sleep(Duration::from_millis(500));

    println!("done: all public-facing functions exercised successfully");

    // Keep the initialized engine live until the end of the smoke test.
    let _ = engine;

    Ok(())
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
            "Usage: cargo run --bin smoke_release_assets -- [asset_dir] [query] [lat] [lon]".into(),
        );
    }

    Ok((asset_dir, query, lat, lon))
}

fn print_all_paths(label: &str, paths: &geo_engine::AllAssetPaths) {
    println!("  {}:", label);
    println!("    geo_db: {}", paths.geo_db_path.display());
    println!(
        "    subdistrict_db: {}",
        paths.subdistrict_db_path.display()
    );
    println!("    city_fst: {}", paths.city_fst_path.display());
    println!("    city_rkyv: {}", paths.city_rkyv_path.display());
    println!("    city_points: {}", paths.city_points_path.display());
}

fn print_city_paths(label: &str, paths: &geo_engine::CityAssetPaths) {
    println!("  {}:", label);
    println!("    fst: {}", paths.fst_path.display());
    println!("    rkyv: {}", paths.rkyv_path.display());
    println!("    points: {}", paths.points_path.display());
}

fn print_reverse(label: &str, result: &geo_engine::ReverseGeocodingResult) {
    println!("  {}:", label);
    println!(
        "    country: {} ({})",
        result.country.name, result.country.iso2
    );
    if let Some(state) = &result.state {
        println!("    state: {} ({})", state.name, state.iso2);
    }
    if let Some(district) = &result.district {
        println!("    district: {} ({})", district.name, district.iso2);
    }
    if let Some(subdistrict) = &result.subdistrict {
        println!(
            "    subdistrict: {} ({})",
            subdistrict.name, subdistrict.iso2
        );
    }
    println!(
        "    city: {} ({})",
        result.city.name, result.city.country_code
    );
}

fn print_search(label: &str, result: &geo_engine::CombinedSearchResult) {
    println!("  {}:", label);
    println!("    cities: {}", result.cities.len());
    println!("    subdistricts: {}", result.subdistricts.len());
}
