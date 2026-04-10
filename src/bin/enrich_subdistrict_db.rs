use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use geo_engine::{find_district_profile, load_district_profiles};
use rkyv::{Archive, Deserialize, Serialize, rancor::Error as RkyvError, to_bytes};

#[derive(Archive, Serialize, Deserialize, Debug)]
struct GeoDB {
    countries: Vec<Country>,
}

#[derive(Archive, Serialize, Deserialize, Debug)]
struct Country {
    name: String,
    iso2: [u8; 2],
    bbox: [f32; 4],
    polygons: Vec<Vec<(f32, f32)>>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let db_path = args.next().unwrap_or_else(|| "subdistrict.db".to_string());
    let csv_path = args.next().unwrap_or_else(|| "data.csv".to_string());

    if args.next().is_some() {
        return Err(
            "usage: cargo run --bin enrich_subdistrict_db -- [subdistrict.db path] [data.csv path]"
                .into(),
        );
    }

    let profiles = load_district_profiles(Path::new(&csv_path))?;
    let bytes = load_db_bytes(Path::new(&db_path))?;
    let archived: &rkyv::Archived<GeoDB> =
        rkyv::access::<rkyv::Archived<GeoDB>, rkyv::rancor::Error>(&bytes)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(&bytes) });

    let mut countries = Vec::with_capacity(archived.countries.len());
    let mut enriched = 0usize;

    for feature in archived.countries.iter() {
        let name = feature.name.to_string();
        let updated_name = match parse_payload(&name) {
            Some(payload) => {
                let demographics = find_district_profile(
                    &profiles,
                    &payload.district_code,
                    &payload.district_name,
                );
                if demographics.is_some() {
                    enriched += 1;
                }
                encode_payload(&payload, demographics)
            }
            None => name,
        };

        countries.push(Country {
            name: updated_name,
            iso2: [feature.iso2[0], feature.iso2[1]],
            bbox: [
                feature.bbox[0].to_native(),
                feature.bbox[1].to_native(),
                feature.bbox[2].to_native(),
                feature.bbox[3].to_native(),
            ],
            polygons: feature
                .polygons
                .iter()
                .map(|ring| {
                    ring.iter()
                        .map(|point| (point.0.to_native(), point.1.to_native()))
                        .collect()
                })
                .collect(),
        });
    }

    let db = GeoDB { countries };
    let serialized = to_bytes::<RkyvError>(&db)?;
    let compressed = zstd::stream::encode_all(&serialized[..], 19)?;
    let output = if compressed.len() < serialized.len() {
        compressed
    } else {
        serialized.to_vec()
    };

    fs::write(&db_path, output)?;
    println!("Updated {}", PathBuf::from(&db_path).display());
    println!("Enriched {enriched} subdistrict records");
    Ok(())
}

fn load_db_bytes(path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    if is_zstd(&bytes) {
        return Ok(zstd::stream::decode_all(&bytes[..])?);
    }
    Ok(bytes)
}

fn is_zstd(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == [0x28, 0xB5, 0x2F, 0xFD]
}

struct Payload {
    subdistrict_name: String,
    district_name: String,
    state_name: String,
    subdistrict_code: String,
    district_code: String,
    state_code: String,
}

fn parse_payload(raw: &str) -> Option<Payload> {
    let parts: Vec<&str> = raw.split("||").collect();
    if parts.len() < 6 {
        return None;
    }

    Some(Payload {
        subdistrict_name: parts[0].trim().to_string(),
        district_name: parts[1].trim().to_string(),
        state_name: parts[2].trim().to_string(),
        subdistrict_code: parts[3].trim().to_string(),
        district_code: parts[4].trim().to_string(),
        state_code: parts[5].trim().to_string(),
    })
}

fn encode_payload(payload: &Payload, demographics: Option<&geo_engine::DistrictProfile>) -> String {
    let mut parts = vec![
        sanitize_field(&payload.subdistrict_name),
        sanitize_field(&payload.district_name),
        sanitize_field(&payload.state_name),
        sanitize_field(&payload.subdistrict_code),
        sanitize_field(&payload.district_code),
        sanitize_field(&payload.state_code),
    ];

    if let Some(profile) = demographics {
        parts.push(sanitize_field(&profile.district_uni_code));
        parts.push(sanitize_field(&profile.major_religion));
        parts.push(
            profile
                .languages
                .iter()
                .map(|language| {
                    format!(
                        "{}~~{}~~{}",
                        sanitize_field(&language.name),
                        sanitize_field(&language.usage_type),
                        sanitize_field(&language.code)
                    )
                })
                .collect::<Vec<String>>()
                .join("##"),
        );
    }

    parts.join("||")
}

fn sanitize_field(value: &str) -> String {
    value
        .replace("||", "|")
        .replace("##", "#")
        .replace("~~", "~")
}
