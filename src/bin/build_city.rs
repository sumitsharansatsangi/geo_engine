use fst::MapBuilder;
use geo_engine::engine::city::{City, CityPoint, normalize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor};
use zip::ZipArchive;

// ----------- MAIN -----------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ---- FST ----
    let fst_file = File::create("cities.fst")?;
    let mut fst = MapBuilder::new(fst_file)?;
    let mut city_keys: BTreeMap<String, u64> = BTreeMap::new();

    // ---- DOWNLOAD ----
    let bytes = reqwest::blocking::get("https://download.geonames.org/export/dump/cities500.zip")?
        .bytes()?;

    // ---- ZIP READ ----
    let reader = Cursor::new(bytes);
    let mut zip = ZipArchive::new(reader)?;
    let file = zip.by_name("cities500.txt")?;
    let buf = BufReader::new(file);

    let mut cities: Vec<City> = Vec::new();
    let mut points: Vec<CityPoint> = Vec::new();

    // ---- PARSE ----
    for (id, line) in buf.lines().enumerate() {
        let line = line?;
        let mut p = line.split('\t');

        p.next(); // skip geonameid

        let name = p.next().unwrap_or("");
        let ascii = p.next().unwrap_or("");
        let alt = p.next().unwrap_or("");
        let lat: f32 = p.next().unwrap_or("0").parse().unwrap_or(0.0);
        let lon: f32 = p.next().unwrap_or("0").parse().unwrap_or(0.0);

        // ---- STORE CITY ----
        cities.push(City {
            name: name.to_string(),
            ascii: ascii.to_string(),
            alternates: alt.split(',').map(|s| s.to_string()).collect(),
            lat,
            lon,
        });

        // ---- STORE POINT ----
        points.push(CityPoint {
            id: id as u32,
            lat,
            lon,
        });

        // ---- FST ----
        collect_key(&mut city_keys, normalize(name), id as u64);
        collect_key(&mut city_keys, normalize(ascii), id as u64);

        for a in alt.split(',') {
            if !a.is_empty() {
                collect_key(&mut city_keys, normalize(a), id as u64);
            }
        }
    }

    for (key, value) in city_keys {
        fst.insert(key, value)?;
    }

    fst.finish()?;

    // ---- SAVE RKYV (Cities) ----
    let city_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cities)?;
    std::fs::write("cities.rkyv", &city_bytes)?;

    // ---- SAVE POINTS (NOT RTREE!) ----
    let point_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&points)?;
    std::fs::write("cities.points", &point_bytes)?;

    println!("✅ Build complete:");
    println!("  - cities.fst");
    println!("  - cities.rkyv");
    println!("  - cities.points");

    Ok(())
}

fn collect_key(city_keys: &mut BTreeMap<String, u64>, key: String, value: u64) {
    if key.is_empty() {
        return;
    }

    city_keys.entry(key).or_insert(value);
}
