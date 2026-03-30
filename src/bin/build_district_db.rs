use std::collections::BTreeMap;
use std::error::Error;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};
use std::fs;
use std::io;
use std::path::Path;

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

#[derive(Debug)]
struct DbfRecord {
    state: String,
    district: String,
}

#[derive(Debug)]
struct ShapeRecord {
    rings: Vec<Vec<(f64, f64)>>,
}

#[derive(Debug)]
struct DistrictFeature {
    name: String,
    iso2: [u8; 2],
    polygons: Vec<Vec<(f32, f32)>>,
    bbox: [f32; 4],
}

#[derive(Debug)]
struct LambertConformalConic {
    a: f64,
    e: f64,
    lon0: f64,
    false_easting: f64,
    false_northing: f64,
    n: f64,
    f: f64,
    rho0: f64,
}

impl LambertConformalConic {
    fn india_lcc() -> Self {
        let a = 6_378_137.0;
        let inv_f = 298.257_223_563;
        let f_flat: f64 = 1.0 / inv_f;
        let e = (2.0 * f_flat - f_flat * f_flat).sqrt();

        let phi1 = 12.472_944_f64.to_radians();
        let phi2 = 35.172_806_f64.to_radians();
        let phi0 = 24.0_f64.to_radians();
        let lon0 = 80.0_f64.to_radians();
        let false_easting = 4_000_000.0;
        let false_northing = 4_000_000.0;

        let m1 = m(phi1, e);
        let m2 = m(phi2, e);
        let t1 = t(phi1, e);
        let t2 = t(phi2, e);
        let t0 = t(phi0, e);

        let n = (m1.ln() - m2.ln()) / (t1.ln() - t2.ln());
        let f = m1 / (n * t1.powf(n));
        let rho0 = a * f * t0.powf(n);

        Self {
            a,
            e,
            lon0,
            false_easting,
            false_northing,
            n,
            f,
            rho0,
        }
    }

    fn inverse(&self, x: f64, y: f64) -> (f64, f64) {
        let dx = x - self.false_easting;
        let dy = self.rho0 - (y - self.false_northing);
        let rho = dx.hypot(dy).copysign(self.n);
        let theta = dx.atan2(dy);
        let t_val = (rho / (self.a * self.f)).powf(1.0 / self.n);

        let mut phi = FRAC_PI_2 - 2.0 * t_val.atan();
        for _ in 0..10 {
            let esin = self.e * phi.sin();
            let next = FRAC_PI_2
                - 2.0
                    * (t_val * ((1.0 - esin) / (1.0 + esin)).powf(self.e / 2.0)).atan();
            if (next - phi).abs() < 1e-12 {
                phi = next;
                break;
            }
            phi = next;
        }

        let lambda = self.lon0 + theta / self.n;
        (lambda.to_degrees(), phi.to_degrees())
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let shp_path = "/Users/amitsharan/Downloads/91/DISTRICT_BOUNDARY.shp";
    let dbf_path = "/Users/amitsharan/Downloads/91/DISTRICT_BOUNDARY.dbf";
    let output_path = "/Users/amitsharan/rustProject/geo_engine/district_in.db";

    let dbf_records = read_dbf_records(Path::new(dbf_path))?;
    let shape_records = read_shape_records(Path::new(shp_path))?;

    if dbf_records.len() != shape_records.len() {
        return Err(format!(
            "shape/attribute count mismatch: {} shapes vs {} rows",
            shape_records.len(),
            dbf_records.len()
        )
        .into());
    }

    let projector = LambertConformalConic::india_lcc();
    let mut districts: BTreeMap<(String, String), DistrictFeature> = BTreeMap::new();

    for (record, shape) in dbf_records.into_iter().zip(shape_records) {
        if record.district.is_empty()
            || record.state.is_empty()
            || record.district.eq_ignore_ascii_case("NOT AVAILABLE")
        {
            continue;
        }

        let mut polygons = select_outer_rings(shape.rings);
        if polygons.is_empty() {
            continue;
        }

        let entry = districts
            .entry((record.state.clone(), record.district.clone()))
            .or_insert_with(|| DistrictFeature {
                name: record.district.clone(),
                iso2: short_code(&record.district),
                polygons: Vec::new(),
                bbox: [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY],
            });

        for ring in polygons.drain(..) {
            let mut transformed = Vec::with_capacity(ring.len());
            for (x, y) in ring {
                let (lon, lat) = projector.inverse(x, y);
                let lon = lon as f32;
                let lat = lat as f32;
                entry.bbox[0] = entry.bbox[0].min(lon);
                entry.bbox[1] = entry.bbox[1].min(lat);
                entry.bbox[2] = entry.bbox[2].max(lon);
                entry.bbox[3] = entry.bbox[3].max(lat);
                transformed.push((lon, lat));
            }
            if transformed.len() >= 3 {
                entry.polygons.push(transformed);
            }
        }
    }

    let countries = districts
        .into_values()
        .filter(|district| !district.polygons.is_empty())
        .map(|district| Country {
            name: district.name,
            iso2: district.iso2,
            bbox: district.bbox,
            polygons: district.polygons,
        })
        .collect();

    let db = GeoDB { countries };
    let bytes = to_bytes::<RkyvError>(&db)?;
    fs::write(output_path, &bytes)?;

    println!("wrote {output_path}");
    Ok(())
}

fn read_dbf_records(path: &Path) -> Result<Vec<DbfRecord>, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    if bytes.len() < 33 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "dbf file too small").into());
    }

    let record_count = read_u32_le(&bytes, 4)? as usize;
    let header_len = read_u16_le(&bytes, 8)? as usize;
    let record_len = read_u16_le(&bytes, 10)? as usize;

    let mut fields = Vec::new();
    let mut offset = 32usize;
    while offset + 32 <= bytes.len() {
        if bytes[offset] == 0x0D {
            offset += 1;
            break;
        }

        let name_end = bytes[offset..offset + 11]
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(11);
        let name = String::from_utf8_lossy(&bytes[offset..offset + name_end]).to_string();
        let length = bytes[offset + 16] as usize;
        fields.push((name, length));
        offset += 32;
    }

    if offset != header_len {
        offset = header_len;
    }

    let state_idx = fields
        .iter()
        .position(|(name, _)| name == "STATE_UT")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing STATE_UT field"))?;
    let district_idx = fields
        .iter()
        .position(|(name, _)| name == "DISTRICT")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing DISTRICT field"))?;

    let mut records = Vec::with_capacity(record_count);
    for _ in 0..record_count {
        if offset + record_len > bytes.len() {
            break;
        }

        let record = &bytes[offset..offset + record_len];
        offset += record_len;

        if record.first() == Some(&b'*') {
            continue;
        }

        let mut field_offset = 1usize;
        let mut state = String::new();
        let mut district = String::new();

        for (index, (_, length)) in fields.iter().enumerate() {
            let end = field_offset + *length;
            if end > record.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "dbf record shorter than expected",
                )
                .into());
            }

            let value = decode_dbf_string(&record[field_offset..end]);
            if index == state_idx {
                state = value;
            } else if index == district_idx {
                district = value;
            }
            field_offset = end;
        }

        records.push(DbfRecord { state, district });
    }

    Ok(records)
}

fn read_shape_records(path: &Path) -> Result<Vec<ShapeRecord>, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    if bytes.len() < 100 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "shp file too small").into());
    }

    let shape_type = read_i32_le(&bytes, 32)?;
    if shape_type != 5 && shape_type != 15 {
        return Err(format!("unsupported shape type {shape_type}, expected polygon/polygonz").into());
    }

    let mut offset = 100usize;
    let mut records = Vec::new();

    while offset + 8 <= bytes.len() {
        let content_len_words = read_i32_be(&bytes, offset + 4)? as usize;
        let content_len = content_len_words * 2;
        let content_start = offset + 8;
        let content_end = content_start + content_len;
        if content_end > bytes.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated shp record").into());
        }

        let record_shape_type = read_i32_le(&bytes, content_start)?;
        if record_shape_type == 0 {
            records.push(ShapeRecord { rings: Vec::new() });
            offset = content_end;
            continue;
        }
        if record_shape_type != 5 && record_shape_type != 15 {
            return Err(format!("unsupported record shape type {record_shape_type}").into());
        }

        let num_parts = read_i32_le(&bytes, content_start + 36)? as usize;
        let num_points = read_i32_le(&bytes, content_start + 40)? as usize;
        let parts_start = content_start + 44;
        let points_start = parts_start + num_parts * 4;
        let points_end = points_start + num_points * 16;
        if points_end > content_end {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "polygon points exceed record bounds",
            )
            .into());
        }

        let mut part_indices = Vec::with_capacity(num_parts + 1);
        for index in 0..num_parts {
            part_indices.push(read_i32_le(&bytes, parts_start + index * 4)? as usize);
        }
        part_indices.push(num_points);

        let mut points = Vec::with_capacity(num_points);
        for index in 0..num_points {
            let point_offset = points_start + index * 16;
            let x = read_f64_le(&bytes, point_offset)?;
            let y = read_f64_le(&bytes, point_offset + 8)?;
            points.push((x, y));
        }

        let mut rings = Vec::with_capacity(num_parts);
        for pair in part_indices.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start >= end || end > points.len() {
                continue;
            }
            rings.push(points[start..end].to_vec());
        }

        records.push(ShapeRecord { rings });
        offset = content_end;
    }

    Ok(records)
}

fn select_outer_rings(rings: Vec<Vec<(f64, f64)>>) -> Vec<Vec<(f64, f64)>> {
    let mut negative_area_rings = Vec::new();
    let mut all_rings = Vec::new();

    for ring in rings {
        if ring.len() < 3 {
            continue;
        }
        let area = signed_area(&ring);
        if area < 0.0 {
            negative_area_rings.push(ring.clone());
        }
        all_rings.push(ring);
    }

    if negative_area_rings.is_empty() {
        all_rings
    } else {
        negative_area_rings
    }
}

fn signed_area(ring: &[(f64, f64)]) -> f64 {
    let mut area = 0.0;
    for window in ring.windows(2) {
        area += window[0].0 * window[1].1 - window[1].0 * window[0].1;
    }

    let first = ring.first().expect("ring is non-empty");
    let last = ring.last().expect("ring is non-empty");
    area += last.0 * first.1 - first.0 * last.1;
    area / 2.0
}

fn decode_dbf_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_matches(char::from(0))
        .trim()
        .to_string()
}

fn short_code(name: &str) -> [u8; 2] {
    let mut code = [b' '; 2];
    let mut chars = name
        .bytes()
        .filter(|b| b.is_ascii_alphabetic())
        .map(|b| b.to_ascii_uppercase());

    if let Some(first) = chars.next() {
        code[0] = first;
    }
    if let Some(second) = chars.next() {
        code[1] = second;
    }

    code
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, io::Error> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "expected u16"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, io::Error> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "expected u32"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_i32_le(bytes: &[u8], offset: usize) -> Result<i32, io::Error> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "expected i32"))?;
    Ok(i32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_i32_be(bytes: &[u8], offset: usize) -> Result<i32, io::Error> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "expected big-endian i32"))?;
    Ok(i32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_f64_le(bytes: &[u8], offset: usize) -> Result<f64, io::Error> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "expected f64"))?;
    Ok(f64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

fn m(phi: f64, e: f64) -> f64 {
    phi.cos() / (1.0 - e * e * phi.sin().powi(2)).sqrt()
}

fn t(phi: f64, e: f64) -> f64 {
    (FRAC_PI_4 - phi / 2.0).tan() / ((1.0 - e * phi.sin()) / (1.0 + e * phi.sin())).powf(e / 2.0)
}
