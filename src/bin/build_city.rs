use fst::MapBuilder;
use geo_engine::engine::city::{City, normalize};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor};
use zip::ZipArchive;

// ── CityPoint: only used for building city index files ──
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone, Copy)]
struct CityPoint {
    id: u32,
    lat: f32,
    lon: f32,
}

// ----------- MAIN -----------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ---- FST ----
    let fst_file = File::create("cities.fst")?;
    let mut fst = MapBuilder::new(fst_file)?;
    let mut city_keys: BTreeMap<String, u64> = BTreeMap::new();

    let admin1_lookup = load_admin1_lookup()?;
    let admin2_lookup = load_admin2_lookup()?;

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
    for line in buf.lines() {
        let line = line?;
        let mut p = line.split('\t');

        let geoname_id: u32 = p.next().unwrap_or("0").parse().unwrap_or(0);
        let name = p.next().unwrap_or("");
        let ascii = p.next().unwrap_or("");
        let alt = p.next().unwrap_or("");
        let lat: f32 = p.next().unwrap_or("0").parse().unwrap_or(0.0);
        let lon: f32 = p.next().unwrap_or("0").parse().unwrap_or(0.0);
        let _feature_class = p.next().unwrap_or("");
        let _feature_code = p.next().unwrap_or("");
        let country_code = p.next().unwrap_or("");
        let _cc2 = p.next().unwrap_or("");
        let admin1_code = normalize_optional(p.next().unwrap_or(""));
        let admin2_code = normalize_optional(p.next().unwrap_or(""));

        let admin1_name = admin1_code.as_ref().and_then(|code| {
            admin1_lookup
                .get(&admin1_lookup_key(country_code, code))
                .cloned()
        });
        let admin2_name = match (&admin1_code, &admin2_code) {
            (Some(admin1_code), Some(admin2_code)) => admin2_lookup
                .get(&admin2_lookup_key(country_code, admin1_code, admin2_code))
                .cloned(),
            _ => None,
        };

        // ---- STORE POINT ----
        points.push(CityPoint {
            id: geoname_id,
            lat,
            lon,
        });

        // ---- FST ----
        collect_key(
            &mut city_keys,
            city_key(
                country_code,
                admin1_code.as_deref(),
                admin2_code.as_deref(),
                geoname_id,
                name,
            ),
            geoname_id as u64,
        );
        collect_key(
            &mut city_keys,
            city_key(
                country_code,
                admin1_code.as_deref(),
                admin2_code.as_deref(),
                geoname_id,
                ascii,
            ),
            geoname_id as u64,
        );

        for a in alt.split(',') {
            if !a.is_empty() {
                collect_key(
                    &mut city_keys,
                    city_key(
                        country_code,
                        admin1_code.as_deref(),
                        admin2_code.as_deref(),
                        geoname_id,
                        a,
                    ),
                    geoname_id as u64,
                );
            }
        }

        // ---- STORE CITY ----
        cities.push(City {
            geoname_id,
            country_code: country_code.to_string(),
            name: name.to_string(),
            ascii: ascii.to_string(),
            alternates: alt.split(',').map(|s| s.to_string()).collect(),
            admin1_code,
            admin1_name,
            admin2_code,
            admin2_name,
            lat,
            lon,
        });
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

fn city_key(
    country_code: &str,
    admin1_code: Option<&str>,
    admin2_code: Option<&str>,
    geoname_id: u32,
    raw_name: &str,
) -> String {
    let normalized_name = normalize(raw_name);
    if normalized_name.is_empty() {
        return String::new();
    }

    format!(
        "{}|{}|{}|{}|{}",
        normalized_name,
        country_code,
        admin1_code.unwrap_or(""),
        admin2_code.unwrap_or(""),
        geoname_id
    )
}

fn normalize_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn load_admin1_lookup() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let bytes =
        reqwest::blocking::get("https://download.geonames.org/export/dump/admin1CodesASCII.txt")?
            .bytes()?;
    let reader = BufReader::new(Cursor::new(bytes));
    let mut lookup = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let mut parts = line.split('\t');
        let code = parts.next().unwrap_or("").trim();
        let name = parts.next().unwrap_or("").trim();
        if !code.is_empty() && !name.is_empty() {
            lookup.insert(code.to_string(), name.to_string());
        }
    }

    Ok(lookup)
}

fn load_admin2_lookup() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let bytes =
        reqwest::blocking::get("https://download.geonames.org/export/dump/admin2Codes.txt")?
            .bytes()?;
    let reader = BufReader::new(Cursor::new(bytes));
    let mut lookup = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let mut parts = line.split('\t');
        let code = parts.next().unwrap_or("").trim();
        let name = parts.next().unwrap_or("").trim();
        if !code.is_empty() && !name.is_empty() {
            lookup.insert(code.to_string(), name.to_string());
        }
    }

    Ok(lookup)
}

fn admin1_lookup_key(country_code: &str, admin1_code: &str) -> String {
    format!("{}.{}", country_code, admin1_code)
}

fn admin2_lookup_key(country_code: &str, admin1_code: &str, admin2_code: &str) -> String {
    format!("{}.{}.{}", country_code, admin1_code, admin2_code)
}
