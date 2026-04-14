use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};
use std::fs;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rkyv::{Archive, Deserialize, Serialize, rancor::Error as RkyvError, to_bytes};

#[path = "common/district_data.rs"]
mod district_data;
#[allow(dead_code)]
#[path = "common/subdistrict_db.rs"]
mod subdistrict_db;
use district_data::{find_district_profile, load_district_profiles};
use subdistrict_db::{
    Country, GeoDB, SubdistrictFeature, encode_subdistrict_payload, short_code, short_code_str,
};

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

#[derive(Archive, Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct SubdistrictKey {
    state: String,
    district: String,
    subdistrict: String,
}

#[derive(Archive, Serialize, Deserialize, Debug)]
struct PartitionRow {
    key: SubdistrictKey,
    feature: SubdistrictFeature,
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

    let mut dbf_reader = DbfReader::open(Path::new(&dbf_path))?;
    let mut shp_reader = ShpReader::open(Path::new(&shp_path))?;
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
    let partition_size = env::var("DISTRICT_PARTITION_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);

    let mut subdistricts: HashMap<SubdistrictKey, SubdistrictFeature> =
        HashMap::with_capacity(8192);
    let mut partition_paths: Vec<PathBuf> = Vec::new();
    let mut total_input_vertices = 0usize;
    let mut total_output_vertices = 0usize;
    let mut record_index = 0usize;

    loop {
        let dbf_record = dbf_reader.next_record()?;
        let shape_record = shp_reader.next_record()?;

        let (record, shape) = match (dbf_record, shape_record) {
            (Some(record), Some(shape)) => (record, shape),
            (None, None) => break,
            (Some(_), None) | (None, Some(_)) => {
                return Err(
                    format!("shape/attribute mismatch near record {}", record_index).into(),
                );
            }
        };
        record_index += 1;

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
            .entry(SubdistrictKey {
                state: record.state.clone(),
                district: record.district.clone(),
                subdistrict: record.subdistrict.clone(),
            })
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
        if partition_size > 0 && subdistricts.len() >= partition_size {
            spill_partition_to_disk(&mut subdistricts, &mut partition_paths)?;
        }
    }

    if !partition_paths.is_empty() {
        if !subdistricts.is_empty() {
            spill_partition_to_disk(&mut subdistricts, &mut partition_paths)?;
        }

        for path in partition_paths {
            load_partition_from_disk(&path, &mut subdistricts)?;
        }
    }

    let mut ordered_subdistricts: Vec<_> = subdistricts.into_iter().collect();
    ordered_subdistricts.sort_unstable_by(|(left_key, _), (right_key, _)| left_key.cmp(right_key));

    let countries = ordered_subdistricts
        .into_iter()
        .map(|(_, subdistrict)| subdistrict)
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
    if partition_size > 0 {
        println!(
            "ℹ️ partition config: DISTRICT_PARTITION_SIZE={}",
            partition_size
        );
    }
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

struct DbfReader {
    reader: BufReader<File>,
    fields: Vec<(String, usize)>,
    record_len: usize,
    remaining_records: usize,
    state_idx: Option<usize>,
    district_idx: Option<usize>,
    subdistrict_idx: Option<usize>,
    state_code_idx: Option<usize>,
    fid_idx: Option<usize>,
}

impl DbfReader {
    fn open(path: &Path) -> Result<Self, Box<dyn Error>> {
        let mut reader = BufReader::new(File::open(path)?);
        let mut header = [0u8; 32];
        reader.read_exact(&mut header)?;

        let record_count = read_u32_le(&header, 4)? as usize;
        let header_len = read_u16_le(&header, 8)? as usize;
        let record_len = read_u16_le(&header, 10)? as usize;

        let mut fields = Vec::new();
        loop {
            let mut first = [0u8; 1];
            reader.read_exact(&mut first)?;
            if first[0] == 0x0D {
                break;
            }

            let mut rest = [0u8; 31];
            reader.read_exact(&mut rest)?;

            let mut descriptor = [0u8; 32];
            descriptor[0] = first[0];
            descriptor[1..].copy_from_slice(&rest);

            let name_end = descriptor[0..11].iter().position(|b| *b == 0).unwrap_or(11);
            let name = String::from_utf8_lossy(&descriptor[0..name_end]).to_string();
            let length = descriptor[16] as usize;
            fields.push((name, length));
        }

        let current = reader.stream_position()? as usize;
        if current < header_len {
            reader.seek(SeekFrom::Start(header_len as u64))?;
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

        Ok(Self {
            reader,
            fields,
            record_len,
            remaining_records: record_count,
            state_idx,
            district_idx,
            subdistrict_idx,
            state_code_idx,
            fid_idx,
        })
    }

    fn next_record(&mut self) -> Result<Option<DbfRecord>, Box<dyn Error>> {
        if self.remaining_records == 0 {
            return Ok(None);
        }

        let mut record = vec![0u8; self.record_len];
        self.reader.read_exact(&mut record)?;
        self.remaining_records -= 1;

        if record.first() == Some(&b'*') {
            return Ok(Some(DbfRecord {
                state: String::new(),
                state_code: String::new(),
                district: String::new(),
                district_code: String::new(),
                subdistrict: String::new(),
                subdistrict_code: String::new(),
            }));
        }

        let mut field_offset = 1usize;
        let mut state = String::new();
        let mut state_code = String::new();
        let mut district = String::new();
        let mut subdistrict = String::new();
        let mut fid = String::new();

        for (index, (_, length)) in self.fields.iter().enumerate() {
            let end = field_offset + *length;
            if end > record.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "dbf record shorter than expected",
                )
                .into());
            }

            let value = decode_dbf_string(&record[field_offset..end]);
            if Some(index) == self.state_idx {
                state = value;
            } else if Some(index) == self.state_code_idx {
                state_code = value;
            } else if Some(index) == self.district_idx {
                district = value;
            } else if Some(index) == self.subdistrict_idx {
                subdistrict = value;
            } else if Some(index) == self.fid_idx {
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

        Ok(Some(DbfRecord {
            state,
            state_code,
            district,
            district_code,
            subdistrict,
            subdistrict_code,
        }))
    }
}

struct ShpReader {
    reader: BufReader<File>,
}

impl ShpReader {
    fn open(path: &Path) -> Result<Self, Box<dyn Error>> {
        let mut reader = BufReader::new(File::open(path)?);
        let mut header = [0u8; 100];
        reader.read_exact(&mut header)?;

        let shape_type = read_i32_le(&header, 32)?;
        if shape_type != 5 && shape_type != 15 {
            return Err(
                format!("unsupported shape type {shape_type}, expected polygon/polygonz").into(),
            );
        }

        Ok(Self { reader })
    }

    fn next_record(&mut self) -> Result<Option<ShapeRecord>, Box<dyn Error>> {
        let mut record_header = [0u8; 8];
        match self.reader.read_exact(&mut record_header) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(err) => return Err(err.into()),
        }

        let content_len_words = i32::from_be_bytes([
            record_header[4],
            record_header[5],
            record_header[6],
            record_header[7],
        ]);
        if content_len_words < 0 {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "negative SHP content length").into(),
            );
        }

        let content_len = (content_len_words as usize) * 2;
        let mut content = vec![0u8; content_len];
        self.reader.read_exact(&mut content)?;

        let record_shape_type = read_i32_le(&content, 0)?;
        if record_shape_type == 0 {
            return Ok(Some(ShapeRecord { rings: Vec::new() }));
        }
        if record_shape_type != 5 && record_shape_type != 15 {
            return Err(format!("unsupported record shape type {record_shape_type}").into());
        }

        let num_parts = read_i32_le(&content, 36)? as usize;
        let num_points = read_i32_le(&content, 40)? as usize;
        let parts_start = 44;
        let points_start = parts_start + num_parts * 4;
        let points_end = points_start + num_points * 16;
        if points_end > content.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "polygon points exceed record bounds",
            )
            .into());
        }

        let mut part_indices = Vec::with_capacity(num_parts + 1);
        for index in 0..num_parts {
            part_indices.push(read_i32_le(&content, parts_start + index * 4)? as usize);
        }
        part_indices.push(num_points);

        let mut rings = Vec::with_capacity(num_parts);
        for pair in part_indices.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start >= end || end > num_points {
                continue;
            }
            let mut ring = Vec::with_capacity(end - start);
            for point_index in start..end {
                let point_offset = points_start + point_index * 16;
                let x = read_f64_le(&content, point_offset)?;
                let y = read_f64_le(&content, point_offset + 8)?;
                ring.push((x, y));
            }
            rings.push(ring);
        }

        Ok(Some(ShapeRecord { rings }))
    }
}

fn spill_partition_to_disk(
    subdistricts: &mut HashMap<SubdistrictKey, SubdistrictFeature>,
    partition_paths: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    if subdistricts.is_empty() {
        return Ok(());
    }

    let partition_path = env::temp_dir().join(format!(
        "geo_engine_subdistrict_partition_{}_{}.bin",
        std::process::id(),
        partition_paths.len()
    ));
    let mut rows = Vec::with_capacity(subdistricts.len());
    for (key, feature) in subdistricts.drain() {
        rows.push(PartitionRow { key, feature });
    }
    let bytes = to_bytes::<RkyvError>(&rows)?;
    fs::write(&partition_path, &bytes)?;

    partition_paths.push(partition_path);
    Ok(())
}

fn load_partition_from_disk(
    partition_path: &Path,
    subdistricts: &mut HashMap<SubdistrictKey, SubdistrictFeature>,
) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(partition_path)?;
    let rows: &rkyv::Archived<Vec<PartitionRow>> =
        rkyv::access::<rkyv::Archived<Vec<PartitionRow>>, rkyv::rancor::Error>(&bytes)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(&bytes) });

    for row in rows.iter() {
        let key = SubdistrictKey {
            state: row.key.state.as_str().to_string(),
            district: row.key.district.as_str().to_string(),
            subdistrict: row.key.subdistrict.as_str().to_string(),
        };

        let incoming = SubdistrictFeature {
            name: row.feature.name.as_str().to_string(),
            code: [row.feature.code[0], row.feature.code[1]],
            polygons: row
                .feature
                .polygons
                .iter()
                .map(|ring| {
                    ring.iter()
                        .map(|point| (point.0.to_native(), point.1.to_native()))
                        .collect()
                })
                .collect(),
            bbox: [
                row.feature.bbox[0].to_native(),
                row.feature.bbox[1].to_native(),
                row.feature.bbox[2].to_native(),
                row.feature.bbox[3].to_native(),
            ],
        };

        if let Some(existing) = subdistricts.get_mut(&key) {
            merge_subdistrict_feature(existing, incoming);
        } else {
            subdistricts.insert(key, incoming);
        }
    }

    let _ = fs::remove_file(partition_path);
    Ok(())
}

fn merge_subdistrict_feature(existing: &mut SubdistrictFeature, incoming: SubdistrictFeature) {
    existing.bbox[0] = existing.bbox[0].min(incoming.bbox[0]);
    existing.bbox[1] = existing.bbox[1].min(incoming.bbox[1]);
    existing.bbox[2] = existing.bbox[2].max(incoming.bbox[2]);
    existing.bbox[3] = existing.bbox[3].max(incoming.bbox[3]);
    existing.polygons.extend(incoming.polygons);
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
            negative_area_rings.push(ring);
        } else {
            all_rings.push(ring);
        }
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
