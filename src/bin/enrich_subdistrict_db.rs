use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use rkyv::{rancor::Error as RkyvError, to_bytes};

#[path = "common/district_data.rs"]
mod district_data;
#[allow(dead_code)]
#[path = "common/subdistrict_db.rs"]
mod subdistrict_db;
use district_data::{find_district_profile, load_district_profiles};
use subdistrict_db::{
    Country, GeoDB, encode_subdistrict_payload, is_zstd, parse_subdistrict_payload,
};

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
        let updated_name = match parse_subdistrict_payload(&name) {
            Some(payload) => {
                let demographics = find_district_profile(
                    &profiles,
                    &payload.district_code,
                    &payload.district_name,
                );
                if demographics.is_some() {
                    enriched += 1;
                }
                encode_subdistrict_payload(
                    &payload.subdistrict_name,
                    &payload.district_name,
                    &payload.state_name,
                    &payload.subdistrict_code,
                    &payload.district_code,
                    &payload.state_code,
                    demographics,
                )
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
