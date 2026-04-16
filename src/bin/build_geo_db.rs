use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rkyv::rancor::Error as RkyvError;
use rkyv::to_bytes;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

#[path = "../engine/model.rs"]
mod model;
use model::{Country, GeoDB};

const DEFAULT_GEOJSON_URL: &str =
    "https://raw.githubusercontent.com/datasets/geo-countries/main/data/countries.geojson";
const DEFAULT_VERSION: &str = "0.0.1";

#[derive(Debug)]
enum InputSource {
    Url(String),
    File(PathBuf),
}

#[derive(Deserialize)]
struct FeatureCollection {
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Option<Properties>,
    geometry: Option<Geometry>,
}

#[derive(Deserialize, Default)]
struct Properties {
    #[serde(rename = "ADMIN")]
    admin: Option<String>,
    #[serde(rename = "NAME")]
    name: Option<String>,
    #[serde(rename = "name")]
    name_lower: Option<String>,
    #[serde(rename = "ISO_A2")]
    iso_a2: Option<String>,
    #[serde(rename = "ISO_A2_EH")]
    iso_a2_eh: Option<String>,
    #[serde(rename = "WB_A2")]
    wb_a2: Option<String>,
    #[serde(rename = "ISO2")]
    iso2: Option<String>,
}

#[derive(Deserialize)]
struct Geometry {
    #[serde(rename = "type")]
    geometry_type: String,
    coordinates: Value,
}

#[derive(Default)]
struct BuildStats {
    skipped_missing_geometry: usize,
    skipped_unsupported_geometry: usize,
    skipped_empty_geometry: usize,
    points: usize,
    rings: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (input, version, output_path) = parse_args(env::args().skip(1))?;

    let precision_dp = env::var("GEO_COORD_PRECISION_DP")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(5)
        .min(7);
    let zstd_level = env::var("GEO_ZSTD_LEVEL")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(19);
    let scale = 10f32.powi(precision_dp as i32);

    let bytes = load_geojson_bytes(&input)?;
    let collection: FeatureCollection = serde_json::from_slice(&bytes)?;

    let mut stats = BuildStats::default();
    let mut countries = Vec::with_capacity(collection.features.len());

    for feature in collection.features {
        let Some(geometry) = feature.geometry else {
            stats.skipped_missing_geometry += 1;
            continue;
        };

        let properties = feature.properties.unwrap_or_default();
        let country_name = country_name_from_properties(&properties);
        let iso2 = country_iso2_from_properties(&properties, &country_name);

        let rings = match outer_rings_from_geometry(&geometry, scale) {
            Ok(rings) => rings,
            Err(GeometryParseError::UnsupportedGeometry) => {
                stats.skipped_unsupported_geometry += 1;
                continue;
            }
            Err(GeometryParseError::NoUsableRings) => {
                stats.skipped_empty_geometry += 1;
                continue;
            }
            Err(GeometryParseError::InvalidCoordinate) => {
                stats.skipped_empty_geometry += 1;
                continue;
            }
        };

        let Some(bbox) = bbox_for_rings(&rings) else {
            stats.skipped_empty_geometry += 1;
            continue;
        };

        stats.rings += rings.len();
        stats.points += rings.iter().map(Vec::len).sum::<usize>();

        countries.push(Country {
            name: country_name,
            iso2,
            bbox,
            polygons: rings,
        });
    }

    countries.sort_unstable_by(|left, right| {
        left.iso2
            .cmp(&right.iso2)
            .then_with(|| left.name.cmp(&right.name))
    });

    let db = GeoDB { countries };
    let serialized = to_bytes::<RkyvError>(&db)?;
    let compressed = zstd::stream::encode_all(&serialized[..], zstd_level)?;
    let output_bytes = if compressed.len() < serialized.len() {
        compressed
    } else {
        serialized.to_vec()
    };

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, &output_bytes)?;
    write_sha256_file(&output_path, &output_bytes)?;

    println!("✅ wrote {} (version={})", output_path.display(), version);
    println!(
        "ℹ️ countries={}, rings={}, points={}",
        db.countries.len(),
        stats.rings,
        stats.points
    );
    println!(
        "ℹ️ skipped: missing_geometry={}, unsupported_geometry={}, empty_or_invalid={}",
        stats.skipped_missing_geometry,
        stats.skipped_unsupported_geometry,
        stats.skipped_empty_geometry
    );
    println!(
        "ℹ️ storage: GEO_COORD_PRECISION_DP={}, GEO_ZSTD_LEVEL={}, raw={} bytes, written={} bytes",
        precision_dp,
        zstd_level,
        serialized.len(),
        output_bytes.len()
    );

    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<(InputSource, String, PathBuf), Box<dyn std::error::Error>> {
    let mut input_url: Option<String> = None;
    let mut input_file: Option<PathBuf> = None;
    let mut version: Option<String> = None;
    let mut output_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input-url" => {
                let value = args
                    .next()
                    .ok_or("missing value for --input-url")?
                    .trim()
                    .to_string();
                if value.is_empty() {
                    return Err("--input-url cannot be empty".into());
                }
                input_url = Some(value);
            }
            "--input-file" => {
                let value = args.next().ok_or("missing value for --input-file")?;
                input_file = Some(PathBuf::from(value));
            }
            "--version" => {
                let value = args.next().ok_or("missing value for --version")?;
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err("--version cannot be empty".into());
                }
                version = Some(trimmed.to_string());
            }
            "--output" => {
                let value = args.next().ok_or("missing value for --output")?;
                output_path = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                return Err(format!("unknown argument: {arg}").into());
            }
        }
    }

    if input_url.is_some() && input_file.is_some() {
        return Err("use only one of --input-url or --input-file".into());
    }

    let input = if let Some(path) = input_file {
        InputSource::File(path)
    } else {
        let url = input_url.unwrap_or_else(|| DEFAULT_GEOJSON_URL.to_string());
        InputSource::Url(normalize_geojson_url(&url))
    };

    let version = version.unwrap_or_else(|| DEFAULT_VERSION.to_string());
    let output = output_path.unwrap_or_else(|| PathBuf::from(default_output_name(&version)));
    Ok((input, version, output))
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!(
        "  cargo run --bin build_geo_db -- [--input-url URL | --input-file PATH] [--version X.Y.Z] [--output PATH]"
    );
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  cargo run --bin build_geo_db -- --version 0.0.2");
    eprintln!(
        "  cargo run --bin build_geo_db -- --input-file data/countries.geojson --version 0.0.2"
    );
}

fn default_output_name(version: &str) -> String {
    format!("release-assets/geo-{version}.db")
}

fn normalize_geojson_url(url: &str) -> String {
    let cleaned = url
        .split_once('#')
        .map(|(head, _)| head)
        .unwrap_or(url)
        .split_once('?')
        .map(|(head, _)| head)
        .unwrap_or(url)
        .trim()
        .to_string();

    if !cleaned.contains("github.com") || !cleaned.contains("/blob/") {
        return cleaned;
    }

    let cleaned = cleaned
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let path = cleaned.trim_start_matches("github.com/");
    let mut parts = path.splitn(4, '/');
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    let blob = parts.next().unwrap_or_default();
    let remainder = parts.next().unwrap_or_default();

    if owner.is_empty() || repo.is_empty() || blob != "blob" || remainder.is_empty() {
        return format!("https://{}", path);
    }

    format!("https://raw.githubusercontent.com/{owner}/{repo}/{remainder}")
}

fn load_geojson_bytes(input: &InputSource) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    match input {
        InputSource::File(path) => Ok(fs::read(path)?),
        InputSource::Url(url) => {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()?;
            let response = client.get(url).send()?.error_for_status()?;
            Ok(response.bytes()?.to_vec())
        }
    }
}

fn country_name_from_properties(properties: &Properties) -> String {
    properties
        .admin
        .as_deref()
        .or(properties.name.as_deref())
        .or(properties.name_lower.as_deref())
        .unwrap_or("UNKNOWN")
        .trim()
        .to_string()
}

fn country_iso2_from_properties(properties: &Properties, country_name: &str) -> [u8; 2] {
    let candidates = [
        properties.iso_a2.as_deref(),
        properties.iso_a2_eh.as_deref(),
        properties.wb_a2.as_deref(),
        properties.iso2.as_deref(),
    ];

    for candidate in candidates.into_iter().flatten() {
        if let Some(code) = parse_iso2(candidate) {
            return code;
        }
    }

    derive_iso2_from_name(country_name)
}

fn parse_iso2(value: &str) -> Option<[u8; 2]> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-99" {
        return None;
    }

    let mut chars = trimmed
        .bytes()
        .filter(|byte| byte.is_ascii_alphabetic())
        .map(|byte| byte.to_ascii_uppercase());

    let first = chars.next()?;
    let second = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    Some([first, second])
}

fn derive_iso2_from_name(name: &str) -> [u8; 2] {
    let mut code = [b' '; 2];
    let mut chars = name
        .bytes()
        .filter(|byte| byte.is_ascii_alphabetic())
        .map(|byte| byte.to_ascii_uppercase());

    if let Some(first) = chars.next() {
        code[0] = first;
    }
    if let Some(second) = chars.next() {
        code[1] = second;
    }
    code
}

#[derive(Debug)]
enum GeometryParseError {
    UnsupportedGeometry,
    InvalidCoordinate,
    NoUsableRings,
}

fn outer_rings_from_geometry(
    geometry: &Geometry,
    scale: f32,
) -> Result<Vec<Vec<(f32, f32)>>, GeometryParseError> {
    match geometry.geometry_type.as_str() {
        "Polygon" => {
            let rings = parse_polygon_rings(&geometry.coordinates, scale)?;
            let Some(outer) = pick_outer_ring(rings) else {
                return Err(GeometryParseError::NoUsableRings);
            };
            Ok(vec![outer])
        }
        "MultiPolygon" => {
            let polygons = geometry
                .coordinates
                .as_array()
                .ok_or(GeometryParseError::InvalidCoordinate)?;

            let mut outer_rings = Vec::with_capacity(polygons.len());
            for polygon in polygons {
                let rings = parse_polygon_rings(polygon, scale)?;
                if let Some(outer) = pick_outer_ring(rings) {
                    outer_rings.push(outer);
                }
            }

            if outer_rings.is_empty() {
                return Err(GeometryParseError::NoUsableRings);
            }
            Ok(outer_rings)
        }
        _ => Err(GeometryParseError::UnsupportedGeometry),
    }
}

fn parse_polygon_rings(
    coordinates: &Value,
    scale: f32,
) -> Result<Vec<Vec<(f32, f32)>>, GeometryParseError> {
    let rings = coordinates
        .as_array()
        .ok_or(GeometryParseError::InvalidCoordinate)?;

    let mut parsed = Vec::with_capacity(rings.len());
    for ring in rings {
        let ring_points = parse_ring(ring, scale)?;
        if ring_points.len() >= 3 {
            parsed.push(ring_points);
        }
    }

    Ok(parsed)
}

fn parse_ring(ring: &Value, scale: f32) -> Result<Vec<(f32, f32)>, GeometryParseError> {
    let points = ring
        .as_array()
        .ok_or(GeometryParseError::InvalidCoordinate)?;
    let mut out = Vec::with_capacity(points.len());

    for point in points {
        let coords = point
            .as_array()
            .ok_or(GeometryParseError::InvalidCoordinate)?;
        if coords.len() < 2 {
            return Err(GeometryParseError::InvalidCoordinate);
        }

        let lon = coords[0]
            .as_f64()
            .ok_or(GeometryParseError::InvalidCoordinate)? as f32;
        let lat = coords[1]
            .as_f64()
            .ok_or(GeometryParseError::InvalidCoordinate)? as f32;

        out.push((quantize_coord(lon, scale), quantize_coord(lat, scale)));
    }

    if out.len() >= 2 {
        let first = out[0];
        let last = out[out.len() - 1];
        if first == last {
            out.pop();
        }
    }

    Ok(out)
}

fn pick_outer_ring(rings: Vec<Vec<(f32, f32)>>) -> Option<Vec<(f32, f32)>> {
    rings.into_iter().max_by(|left, right| {
        signed_area(left)
            .abs()
            .partial_cmp(&signed_area(right).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn signed_area(ring: &[(f32, f32)]) -> f32 {
    if ring.len() < 3 {
        return 0.0;
    }

    let mut area = 0.0f32;
    for i in 0..ring.len() {
        let (x1, y1) = ring[i];
        let (x2, y2) = ring[(i + 1) % ring.len()];
        area += x1 * y2 - x2 * y1;
    }

    area * 0.5
}

fn bbox_for_rings(rings: &[Vec<(f32, f32)>]) -> Option<[f32; 4]> {
    let mut min_lon = f32::INFINITY;
    let mut min_lat = f32::INFINITY;
    let mut max_lon = f32::NEG_INFINITY;
    let mut max_lat = f32::NEG_INFINITY;

    let mut seen = false;
    for ring in rings {
        for (lon, lat) in ring {
            min_lon = min_lon.min(*lon);
            min_lat = min_lat.min(*lat);
            max_lon = max_lon.max(*lon);
            max_lat = max_lat.max(*lat);
            seen = true;
        }
    }

    if seen {
        Some([min_lon, min_lat, max_lon, max_lat])
    } else {
        None
    }
}

fn quantize_coord(value: f32, scale: f32) -> f32 {
    (value * scale).round() / scale
}

fn write_sha256_file(
    output_path: &Path,
    output_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut hasher = Sha256::new();
    hasher.update(output_bytes);
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let checksum_path = output_path.with_extension("db.sha256");
    let filename = output_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("geo-0.0.1.db");
    fs::write(checksum_path, format!("{hex}  {filename}\n"))?;

    Ok(())
}
