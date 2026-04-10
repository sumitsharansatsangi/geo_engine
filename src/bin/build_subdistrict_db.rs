use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};
use std::fs;
use std::io;
use std::path::Path;

use geo_engine::{DistrictProfile, find_district_profile, load_district_profiles};
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
    state_code: String,
    district: String,
    district_code: String,
    subdistrict: String,
    subdistrict_code: String,
}

#[derive(Debug)]
struct ShapeRecord {
    rings: Vec<Vec<(f64, f64)>>,
}

#[derive(Debug)]
struct SubdistrictFeature {
    name: String,
    code: [u8; 2],
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
            let next =
                FRAC_PI_2 - 2.0 * (t_val * ((1.0 - esin) / (1.0 + esin)).powf(self.e / 2.0)).atan();
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

pub fn run() -> Result<(), Box<dyn Error>> {
    let shp_path = env::var("SUBDISTRICT_SHP_PATH")
        .unwrap_or_else(|_| "/Users/amitsharan/Downloads/91/SUBDISTRICT_BOUNDARY.shp".to_string());
    let dbf_path = env::var("SUBDISTRICT_DBF_PATH")
        .unwrap_or_else(|_| "/Users/amitsharan/Downloads/91/SUBDISTRICT_BOUNDARY.dbf".to_string());
    let output_path = env::var("SUBDISTRICT_OUTPUT_PATH")
        .unwrap_or_else(|_| "/Users/amitsharan/rustProject/geo_engine/subdistrict.db".to_string());
    let data_csv_path = env::var("DISTRICT_DATA_CSV_PATH")
        .or_else(|_| env::var("DATA_CSV_PATH"))
        .unwrap_or_else(|_| "data.csv".to_string());

    let dbf_records = read_dbf_records(Path::new(&dbf_path))?;
    let shape_records = read_shape_records(Path::new(&shp_path))?;
    let district_profiles = match load_district_profiles(Path::new(&data_csv_path)) {
        Ok(profiles) => {
            println!("ℹ️ loaded district demographics from {data_csv_path}");
            Some(profiles)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            println!(
                "ℹ️ no district demographics CSV found at {data_csv_path}; continuing without it"
            );
            None
        }
        Err(err) => return Err(err.into()),
    };

    if dbf_records.len() != shape_records.len() {
        return Err(format!(
            "shape/attribute mismatch: {} vs {}",
            shape_records.len(),
            dbf_records.len()
        )
        .into());
    }

    let projector = LambertConformalConic::india_lcc();
    let simplify_tolerance_m = env::var("DISTRICT_SIMPLIFY_TOLERANCE_M")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(20.0);
    let min_vertex_spacing_m = env::var("DISTRICT_MIN_VERTEX_SPACING_M")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(1.5);
    let coordinate_precision_dp = env::var("DISTRICT_COORD_PRECISION_DP")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(4);
    let coordinate_scale = 10f32.powi(coordinate_precision_dp.min(7) as i32);
    let zstd_level = env::var("DISTRICT_ZSTD_LEVEL")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(19);

    let mut subdistricts: BTreeMap<(String, String, String), SubdistrictFeature> = BTreeMap::new();
    let mut total_input_vertices = 0usize;
    let mut total_output_vertices = 0usize;

    for (record, shape) in dbf_records.into_iter().zip(shape_records) {
        if record.subdistrict.is_empty()
            || record.district.is_empty()
            || record.state.is_empty()
            || record.subdistrict.eq_ignore_ascii_case("NOT AVAILABLE")
            || record.district.eq_ignore_ascii_case("NOT AVAILABLE")
        {
            continue;
        }

        let mut polygons = select_outer_rings(shape.rings);
        if polygons.is_empty() {
            continue;
        }

        let entry = subdistricts
            .entry((
                record.state.clone(),
                record.district.clone(),
                record.subdistrict.clone(),
            ))
            .or_insert_with(|| SubdistrictFeature {
                name: {
                    let demographics = district_profiles.as_ref().and_then(|profiles| {
                        find_district_profile(profiles, &record.district_code, &record.district)
                    });
                    encode_subdistrict_payload(
                        &record.subdistrict,
                        &record.district,
                        &record.state,
                        &record.subdistrict_code,
                        &record.district_code,
                        &record.state_code,
                        demographics,
                    )
                },
                code: short_code(&record.subdistrict),
                polygons: Vec::new(),
                bbox: [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
            });

        for ring in polygons.drain(..) {
            total_input_vertices += ring.len();
            let ring = simplify_ring(ring, simplify_tolerance_m, min_vertex_spacing_m);
            total_output_vertices += ring.len();
            let mut transformed = Vec::with_capacity(ring.len());

            for (x, y) in ring {
                let (lon, lat) = projector.inverse(x, y);
                let lon = quantize_coord(lon as f32, coordinate_scale);
                let lat = quantize_coord(lat as f32, coordinate_scale);

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

    let countries = subdistricts
        .into_values()
        .filter(|d| !d.polygons.is_empty())
        .map(|d| Country {
            name: d.name,
            iso2: d.code,
            bbox: d.bbox,
            polygons: d.polygons,
        })
        .collect();

    let db = GeoDB { countries };
    let bytes = to_bytes::<RkyvError>(&db)?;
    let compressed = zstd::stream::encode_all(&bytes[..], zstd_level)?;
    let output_bytes = if compressed.len() < bytes.len() {
        compressed
    } else {
        bytes.to_vec()
    };
    fs::write(&output_path, &output_bytes)?;

    println!("✅ wrote {output_path}");
    println!(
        "ℹ️ vertex reduction: {} -> {} ({:.1}% kept)",
        total_input_vertices,
        total_output_vertices,
        if total_input_vertices == 0 {
            0.0
        } else {
            (total_output_vertices as f64 / total_input_vertices as f64) * 100.0
        }
    );
    println!(
        "ℹ️ simplification config: DISTRICT_SIMPLIFY_TOLERANCE_M={}, DISTRICT_MIN_VERTEX_SPACING_M={}",
        simplify_tolerance_m, min_vertex_spacing_m
    );
    println!(
        "ℹ️ storage config: DISTRICT_COORD_PRECISION_DP={}, DISTRICT_ZSTD_LEVEL={}",
        coordinate_precision_dp.min(7),
        zstd_level
    );
    println!(
        "ℹ️ serialized size: raw={} bytes, written={} bytes",
        bytes.len(),
        output_bytes.len()
    );
    Ok(())
}

#[allow(dead_code)]
fn main() -> Result<(), Box<dyn Error>> {
    run()
}

fn quantize_coord(value: f32, scale: f32) -> f32 {
    (value * scale).round() / scale
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

    let state_idx = find_field_index(&fields, &["STATE_UT", "STATE", "ST_NM", "STATE_NAME"]);
    let district_idx = find_field_index(
        &fields,
        &["DISTRICT", "DIST_NAME", "DISTRICT_NM", "DIST_NAME_1"],
    );
    let subdistrict_idx = find_field_index(
        &fields,
        &[
            "SUB_DIST",
            "SUBDISTRICT",
            "SUB_DIST_NM",
            "SUBDIS_NM",
            "SUBDIST_NM",
            "TEHSIL",
            "TALUKA",
            "MANDAL",
            "BLOCK",
        ],
    );
    let state_code_idx = find_field_index(&fields, &["STATE_LGD", "STATE_CODE", "ST_CODE"]);
    let fid_idx = find_field_index(&fields, &["FID", "OBJECTID", "ID"]);

    if subdistrict_idx.is_none() && fid_idx.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing subdistrict field (expected SUB_DIST or FID fallback)",
        )
        .into());
    }

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
        let mut state_code = String::new();
        let mut district = String::new();
        let mut subdistrict = String::new();
        let mut fid = String::new();

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
            if Some(index) == state_idx {
                state = value;
            } else if Some(index) == state_code_idx {
                state_code = value;
            } else if Some(index) == district_idx {
                district = value;
            } else if Some(index) == subdistrict_idx {
                subdistrict = value;
            } else if Some(index) == fid_idx {
                fid = value;
            }
            field_offset = end;
        }

        if state.is_empty() {
            state = "UNKNOWN".to_string();
        }
        if district.is_empty() {
            district = "UNKNOWN".to_string();
        }
        if subdistrict.is_empty() && !fid.is_empty() {
            subdistrict = format!("SUBDISTRICT_{fid}");
        }
        if state_code.is_empty() {
            state_code = short_code_str(&state);
        }
        let district_code = short_code_str(&district);
        let subdistrict_code = short_code_str(&subdistrict);

        records.push(DbfRecord {
            state,
            state_code,
            district,
            district_code,
            subdistrict,
            subdistrict_code,
        });
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
        return Err(
            format!("unsupported shape type {shape_type}, expected polygon/polygonz").into(),
        );
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

fn find_field_index(fields: &[(String, usize)], candidates: &[&str]) -> Option<usize> {
    fields.iter().position(|(name, _)| {
        candidates
            .iter()
            .any(|candidate| name.eq_ignore_ascii_case(candidate))
    })
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

fn short_code_str(name: &str) -> String {
    String::from_utf8_lossy(&short_code(name))
        .trim()
        .to_string()
}

fn encode_subdistrict_payload(
    subdistrict: &str,
    district: &str,
    state: &str,
    subdistrict_code: &str,
    district_code: &str,
    state_code: &str,
    demographics: Option<&DistrictProfile>,
) -> String {
    let mut fields = vec![
        sanitize_field(subdistrict),
        sanitize_field(district),
        sanitize_field(state),
        sanitize_field(subdistrict_code),
        sanitize_field(district_code),
        sanitize_field(state_code),
    ];

    if let Some(profile) = demographics {
        fields.push(sanitize_field(&profile.major_religion));
        fields.push(encode_languages(profile));
    }

    fields.join("||")
}

fn sanitize_field(value: &str) -> String {
    value
        .replace("||", "|")
        .replace("##", "#")
        .replace("~~", "~")
}

fn encode_languages(profile: &DistrictProfile) -> String {
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
        .join("##")
}

fn simplify_ring(
    mut ring: Vec<(f64, f64)>,
    simplify_tolerance_m: f64,
    min_vertex_spacing_m: f64,
) -> Vec<(f64, f64)> {
    if ring.len() < 4 {
        return ring;
    }

    // Many shapefile rings repeat the first point at the end.
    if squared_distance(
        *ring.first().expect("non-empty"),
        *ring.last().expect("non-empty"),
    ) <= min_vertex_spacing_m * min_vertex_spacing_m
    {
        ring.pop();
    }

    if ring.len() < 4 {
        return ring;
    }

    let mut deduped = Vec::with_capacity(ring.len());
    deduped.push(ring[0]);
    for point in ring.into_iter().skip(1) {
        if squared_distance(*deduped.last().expect("non-empty"), point)
            > min_vertex_spacing_m * min_vertex_spacing_m
        {
            deduped.push(point);
        }
    }

    if deduped.len() < 4 || simplify_tolerance_m <= 0.0 {
        return deduped;
    }

    let mut kept = Vec::with_capacity(deduped.len());
    let n = deduped.len();
    let tol_sq = simplify_tolerance_m * simplify_tolerance_m;
    for i in 0..n {
        let prev = deduped[(i + n - 1) % n];
        let curr = deduped[i];
        let next = deduped[(i + 1) % n];
        if point_segment_distance_sq(curr, prev, next) > tol_sq {
            kept.push(curr);
        }
    }

    if kept.len() >= 3 { kept } else { deduped }
}

fn squared_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

fn point_segment_distance_sq(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let abx = b.0 - a.0;
    let aby = b.1 - a.1;
    let apx = p.0 - a.0;
    let apy = p.1 - a.1;
    let ab_len_sq = abx * abx + aby * aby;

    if ab_len_sq == 0.0 {
        return squared_distance(p, a);
    }

    let t = (apx * abx + apy * aby) / ab_len_sq;
    let t = t.clamp(0.0, 1.0);
    let cx = a.0 + t * abx;
    let cy = a.1 + t * aby;
    let dx = p.0 - cx;
    let dy = p.1 - cy;
    dx * dx + dy * dy
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
