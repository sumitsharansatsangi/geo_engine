use std::env;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let countries_db = env::var("COUNTRY_DB_PATH").unwrap_or_else(|_| "geo.db".to_string());
    let subdistrict_db =
        env::var("SUBDISTRICT_DB_PATH").unwrap_or_else(|_| "subdistrict.db".to_string());
    let countries_geojson = env::var("COUNTRY_GEOJSON_PATH")
        .unwrap_or_else(|_| "dart/assets/countries.geojson".to_string());
    let subdistrict_geojson = env::var("SUBDISTRICT_GEOJSON_PATH")
        .unwrap_or_else(|_| "dart/assets/india_subdistricts.geojson".to_string());

    export_countries_geojson(Path::new(&countries_db), Path::new(&countries_geojson))?;
    export_subdistrict_geojson(Path::new(&subdistrict_db), Path::new(&subdistrict_geojson))?;

    println!("✅ wrote {}", countries_geojson);
    println!("✅ wrote {}", subdistrict_geojson);
    Ok(())
}

fn export_countries_geojson(db_path: &Path, out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = load_db_bytes(db_path)?;
    let db: &rkyv::Archived<GeoDB> =
        rkyv::access::<rkyv::Archived<GeoDB>, rkyv::rancor::Error>(&bytes)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(&bytes) });

    ensure_parent_dir(out_path)?;
    let file = fs::File::create(out_path)?;
    let mut w = BufWriter::new(file);

    writeln!(w, "{{\"type\":\"FeatureCollection\",\"features\":[")?;
    for (idx, country) in db.countries.iter().enumerate() {
        if idx > 0 {
            writeln!(w, ",")?;
        }
        write!(
            w,
            "{{\"type\":\"Feature\",\"properties\":{{\"name\":\"{}\",\"iso2\":\"{}\"}},\"geometry\":{{\"type\":\"MultiPolygon\",\"coordinates\":[",
            escape_json(country.name.as_str()),
            iso2_to_string(&country.iso2),
        )?;

        for (poly_idx, ring) in country.polygons.iter().enumerate() {
            if poly_idx > 0 {
                write!(w, ",")?;
            }
            write!(w, "[[")?;
            for (point_idx, point) in ring.iter().enumerate() {
                if point_idx > 0 {
                    write!(w, ",")?;
                }
                write!(w, "[{:.6},{:.6}]", point.0, point.1)?;
            }
            write!(w, "]]")?;
        }

        write!(w, "]}}}}")?;
    }
    writeln!(w, "]}}")?;
    w.flush()?;
    Ok(())
}

fn export_subdistrict_geojson(
    db_path: &Path,
    out_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = load_db_bytes(db_path)?;
    let db: &rkyv::Archived<GeoDB> =
        rkyv::access::<rkyv::Archived<GeoDB>, rkyv::rancor::Error>(&bytes)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(&bytes) });

    ensure_parent_dir(out_path)?;
    let file = fs::File::create(out_path)?;
    let mut w = BufWriter::new(file);

    writeln!(w, "{{\"type\":\"FeatureCollection\",\"features\":[")?;
    for (idx, feature) in db.countries.iter().enumerate() {
        if idx > 0 {
            writeln!(w, ",")?;
        }

        let payload = parse_subdistrict_payload(feature.name.as_str());
        write!(
            w,
            "{{\"type\":\"Feature\",\"properties\":{{\"SUB_DIST\":\"{}\",\"DISTRICT\":\"{}\",\"STATE_UT\":\"{}\",\"SUBDIS_LGD\":\"{}\",\"DIST_LGD\":\"{}\",\"STATE_LGD\":\"{}\",\"RAW_NAME\":\"{}\"}},\"geometry\":{{\"type\":\"MultiPolygon\",\"coordinates\":[",
            escape_json(&payload.subdistrict),
            escape_json(&payload.district),
            escape_json(&payload.state),
            escape_json(&payload.subdistrict_code),
            escape_json(&payload.district_code),
            escape_json(&payload.state_code),
            escape_json(feature.name.as_str()),
        )?;

        for (poly_idx, ring) in feature.polygons.iter().enumerate() {
            if poly_idx > 0 {
                write!(w, ",")?;
            }
            write!(w, "[[")?;
            for (point_idx, point) in ring.iter().enumerate() {
                if point_idx > 0 {
                    write!(w, ",")?;
                }
                write!(w, "[{:.6},{:.6}]", point.0, point.1)?;
            }
            write!(w, "]]")?;
        }

        write!(w, "]}}}}")?;
    }
    writeln!(w, "]}}")?;
    w.flush()?;
    Ok(())
}

fn load_db_bytes(path: &Path) -> Result<Vec<u8>, io::Error> {
    let raw = fs::read(path)?;
    if is_zstd(&raw) {
        return zstd::stream::decode_all(&raw[..]).map_err(io::Error::other);
    }
    Ok(raw)
}

fn is_zstd(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == [0x28, 0xB5, 0x2F, 0xFD]
}

fn iso2_to_string(code: &rkyv::Archived<[u8; 2]>) -> String {
    String::from_utf8_lossy(&[code[0], code[1]]).trim().to_string()
}

fn ensure_parent_dir(path: &Path) -> Result<(), io::Error> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

struct SubdistrictPayload {
    subdistrict: String,
    district: String,
    state: String,
    subdistrict_code: String,
    district_code: String,
    state_code: String,
}

fn parse_subdistrict_payload(raw: &str) -> SubdistrictPayload {
    let parts: Vec<&str> = raw.split("||").collect();
    if parts.len() == 6 {
        return SubdistrictPayload {
            subdistrict: parts[0].trim().to_string(),
            district: parts[1].trim().to_string(),
            state: parts[2].trim().to_string(),
            subdistrict_code: parts[3].trim().to_string(),
            district_code: parts[4].trim().to_string(),
            state_code: parts[5].trim().to_string(),
        };
    }

    SubdistrictPayload {
        subdistrict: raw.to_string(),
        district: "UNKNOWN".to_string(),
        state: "UNKNOWN".to_string(),
        subdistrict_code: "UN".to_string(),
        district_code: "UN".to_string(),
        state_code: "UN".to_string(),
    }
}

fn escape_json(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}
