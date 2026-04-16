use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use h3o::{LatLng, Resolution};
use rkyv::{Archive, Deserialize, Serialize};

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

#[derive(Archive, Serialize, Deserialize, Debug)]
struct H3IndexFile {
    resolution: u8,
    cells: Vec<H3CellEntry>,
}

#[derive(Archive, Serialize, Deserialize, Debug)]
struct H3CellEntry {
    cell: u64,
    polygon_ids: Vec<u32>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let geo_db_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("release-assets/geo-0.0.1.db"));
    let output_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_sidecar_path(&geo_db_path));

    if args.next().is_some() {
        eprintln!("Usage: cargo run --bin build_h3_index -- [geo-0.0.1.db path] [output.h3 path]");
        std::process::exit(2);
    }

    let resolution = env::var("H3_RES")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(5);

    let bytes = fs::read(&geo_db_path)?;
    let raw = if is_zstd(&bytes) {
        zstd::stream::decode_all(&bytes[..])?
    } else {
        bytes
    };

    let db: &rkyv::Archived<GeoDB> =
        rkyv::access::<rkyv::Archived<GeoDB>, rkyv::rancor::Error>(&raw).unwrap_or_else(|_| {
            unsafe {
                // SAFETY: geo-0.0.1.db is created by trusted build pipeline for this project.
                rkyv::access_unchecked(&raw)
            }
        });

    let index = build_h3_index(&db.countries, resolution);

    let mut cells: Vec<H3CellEntry> = index
        .into_iter()
        .map(|(cell, mut ids)| {
            ids.sort_unstable();
            ids.dedup();
            H3CellEntry {
                cell,
                polygon_ids: ids,
            }
        })
        .collect();

    cells.sort_by_key(|entry| entry.cell);

    let payload = H3IndexFile { resolution, cells };
    let serialized = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, &serialized)?;

    println!(
        "✅ wrote {} (resolution={}, cells={})",
        output_path.display(),
        resolution,
        payload.cells.len()
    );

    Ok(())
}

fn default_sidecar_path(geo_db_path: &Path) -> PathBuf {
    geo_db_path.with_extension("h3")
}

fn build_h3_index(
    countries: &rkyv::Archived<Vec<Country>>,
    resolution: u8,
) -> HashMap<u64, Vec<u32>> {
    let mut index: HashMap<u64, Vec<u32>> = HashMap::new();

    for (country_id, country) in countries.iter().enumerate() {
        let mut country_cells = HashSet::new();

        for ring in country.polygons.iter() {
            for point in ring.iter() {
                let lon: f32 = point.0.into();
                let lat: f32 = point.1.into();
                if let Some(cell) = point_to_cell(lat, lon, resolution) {
                    country_cells.insert(cell);
                }
            }
        }

        let min_lon: f32 = country.bbox[0].into();
        let min_lat: f32 = country.bbox[1].into();
        let max_lon: f32 = country.bbox[2].into();
        let max_lat: f32 = country.bbox[3].into();

        let bbox_points = [
            (min_lat, min_lon),
            (min_lat, max_lon),
            (max_lat, min_lon),
            (max_lat, max_lon),
            ((min_lat + max_lat) * 0.5, (min_lon + max_lon) * 0.5),
        ];

        for (lat, lon) in bbox_points {
            if let Some(cell) = point_to_cell(lat, lon, resolution) {
                country_cells.insert(cell);
            }
        }

        for cell in country_cells {
            index.entry(cell).or_default().push(country_id as u32);
        }
    }

    index
}

fn point_to_cell(lat: f32, lon: f32, resolution: u8) -> Option<u64> {
    let latlng = LatLng::new(lat as f64, lon as f64).ok()?;
    let res = Resolution::try_from(resolution).ok()?;
    Some(u64::from(latlng.to_cell(res)))
}

fn is_zstd(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == [0x28, 0xB5, 0x2F, 0xFD]
}
