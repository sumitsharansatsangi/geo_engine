use rkyv::Archived;
use std::collections::BTreeSet;

use super::error::GeoEngineError;
use super::index::SpatialIndex;
use super::model::Country;
use super::polygon::point_in_ring;

pub fn find_country<'a>(
    lat: f32,
    lon: f32,
    index: &SpatialIndex,
    countries: &'a Archived<Vec<Country>>,
) -> Result<&'a Archived<Country>, GeoEngineError> {
    let mut checked_countries = BTreeSet::new();
    for (country_id, ring_id) in index.polygon_candidates(lat, lon) {
        let Some(country) = countries.get(country_id as usize) else {
            continue;
        };
        let Some(ring) = country.polygons.get(ring_id as usize) else {
            continue;
        };

        checked_countries.insert(country_id);
        if point_in_ring(lat, lon, ring) {
            return Ok(country);
        }
    }

    let candidate_ids: Vec<u32> = index.candidates(lat, lon).collect();
    let prefiltered_ids = prefilter_bbox_candidates(lat, lon, countries, candidate_ids);

    for country_id in prefiltered_ids {
        if checked_countries.contains(&country_id) {
            continue;
        }
        let Some(country) = countries.get(country_id as usize) else {
            continue;
        };
        for ring in country.polygons.iter() {
            if point_in_ring(lat, lon, ring) {
                return Ok(country);
            }
        }
    }

    Err(GeoEngineError::CountryNotFound { lat, lon })
}

pub fn prefilter_bbox_candidates(
    lat: f32,
    lon: f32,
    countries: &Archived<Vec<Country>>,
    candidate_ids: Vec<u32>,
) -> Vec<u32> {
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return prefilter_bbox_neon(lat, lon, countries, candidate_ids);
        }
    }

    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("sse") {
            return unsafe { prefilter_bbox_sse(lat, lon, countries, candidate_ids) };
        }
    }

    prefilter_bbox_scalar(lat, lon, countries, candidate_ids)
}

fn prefilter_bbox_scalar(
    lat: f32,
    lon: f32,
    countries: &Archived<Vec<Country>>,
    candidate_ids: Vec<u32>,
) -> Vec<u32> {
    let mut filtered = Vec::with_capacity(candidate_ids.len());
    for id in candidate_ids {
        let Some(country) = countries.get(id as usize) else {
            continue;
        };

        let min_lon: f32 = country.bbox[0].into();
        let min_lat: f32 = country.bbox[1].into();
        let max_lon: f32 = country.bbox[2].into();
        let max_lat: f32 = country.bbox[3].into();

        if lon >= min_lon && lon <= max_lon && lat >= min_lat && lat <= max_lat {
            filtered.push(id);
        }
    }
    filtered
}

#[cfg(target_arch = "aarch64")]
fn prefilter_bbox_neon(
    lat: f32,
    lon: f32,
    countries: &Archived<Vec<Country>>,
    candidate_ids: Vec<u32>,
) -> Vec<u32> {
    use std::arch::aarch64::*;

    let mut filtered = Vec::with_capacity(candidate_ids.len());
    let lon_v = unsafe { vdupq_n_f32(lon) };
    let lat_v = unsafe { vdupq_n_f32(lat) };

    let mut chunks = candidate_ids.chunks_exact(4);
    for chunk in &mut chunks {
        let mut min_lon = [0.0f32; 4];
        let mut min_lat = [0.0f32; 4];
        let mut max_lon = [0.0f32; 4];
        let mut max_lat = [0.0f32; 4];

        for lane in 0..4 {
            let Some(country) = countries.get(chunk[lane] as usize) else {
                min_lon[lane] = f32::INFINITY;
                min_lat[lane] = f32::INFINITY;
                max_lon[lane] = f32::NEG_INFINITY;
                max_lat[lane] = f32::NEG_INFINITY;
                continue;
            };
            min_lon[lane] = country.bbox[0].into();
            min_lat[lane] = country.bbox[1].into();
            max_lon[lane] = country.bbox[2].into();
            max_lat[lane] = country.bbox[3].into();
        }

        let min_lon_v = unsafe { vld1q_f32(min_lon.as_ptr()) };
        let min_lat_v = unsafe { vld1q_f32(min_lat.as_ptr()) };
        let max_lon_v = unsafe { vld1q_f32(max_lon.as_ptr()) };
        let max_lat_v = unsafe { vld1q_f32(max_lat.as_ptr()) };

        let lon_ge = unsafe { vcgeq_f32(lon_v, min_lon_v) };
        let lon_le = unsafe { vcleq_f32(lon_v, max_lon_v) };
        let lat_ge = unsafe { vcgeq_f32(lat_v, min_lat_v) };
        let lat_le = unsafe { vcleq_f32(lat_v, max_lat_v) };

        let mask_lon = unsafe { vandq_u32(lon_ge, lon_le) };
        let mask_lat = unsafe { vandq_u32(lat_ge, lat_le) };
        let mask_all = unsafe { vandq_u32(mask_lon, mask_lat) };

        let mut lanes = [0u32; 4];
        unsafe { vst1q_u32(lanes.as_mut_ptr(), mask_all) };

        for lane in 0..4 {
            if lanes[lane] != 0 {
                filtered.push(chunk[lane]);
            }
        }
    }

    for &id in chunks.remainder() {
        let Some(country) = countries.get(id as usize) else {
            continue;
        };
        let min_lon: f32 = country.bbox[0].into();
        let min_lat: f32 = country.bbox[1].into();
        let max_lon: f32 = country.bbox[2].into();
        let max_lat: f32 = country.bbox[3].into();
        if lon >= min_lon && lon <= max_lon && lat >= min_lat && lat <= max_lat {
            filtered.push(id);
        }
    }

    filtered
}

#[cfg(target_arch = "x86_64")]
unsafe fn prefilter_bbox_sse(
    lat: f32,
    lon: f32,
    countries: &Archived<Vec<Country>>,
    candidate_ids: Vec<u32>,
) -> Vec<u32> {
    use std::arch::x86_64::*;

    let mut filtered = Vec::with_capacity(candidate_ids.len());
    let lon_v = _mm_set1_ps(lon);
    let lat_v = _mm_set1_ps(lat);

    let mut chunks = candidate_ids.chunks_exact(4);
    for chunk in &mut chunks {
        let mut min_lon = [0.0f32; 4];
        let mut min_lat = [0.0f32; 4];
        let mut max_lon = [0.0f32; 4];
        let mut max_lat = [0.0f32; 4];

        for lane in 0..4 {
            let Some(country) = countries.get(chunk[lane] as usize) else {
                min_lon[lane] = f32::INFINITY;
                min_lat[lane] = f32::INFINITY;
                max_lon[lane] = f32::NEG_INFINITY;
                max_lat[lane] = f32::NEG_INFINITY;
                continue;
            };
            min_lon[lane] = country.bbox[0].into();
            min_lat[lane] = country.bbox[1].into();
            max_lon[lane] = country.bbox[2].into();
            max_lat[lane] = country.bbox[3].into();
        }

        let min_lon_v = _mm_set_ps(min_lon[3], min_lon[2], min_lon[1], min_lon[0]);
        let min_lat_v = _mm_set_ps(min_lat[3], min_lat[2], min_lat[1], min_lat[0]);
        let max_lon_v = _mm_set_ps(max_lon[3], max_lon[2], max_lon[1], max_lon[0]);
        let max_lat_v = _mm_set_ps(max_lat[3], max_lat[2], max_lat[1], max_lat[0]);

        let lon_ge = _mm_cmpge_ps(lon_v, min_lon_v);
        let lon_le = _mm_cmple_ps(lon_v, max_lon_v);
        let lat_ge = _mm_cmpge_ps(lat_v, min_lat_v);
        let lat_le = _mm_cmple_ps(lat_v, max_lat_v);

        let mask = _mm_and_ps(_mm_and_ps(lon_ge, lon_le), _mm_and_ps(lat_ge, lat_le));
        let bits = _mm_movemask_ps(mask);

        for lane in 0..4 {
            if (bits & (1 << lane)) != 0 {
                filtered.push(chunk[lane]);
            }
        }
    }

    for &id in chunks.remainder() {
        let Some(country) = countries.get(id as usize) else {
            continue;
        };
        let min_lon: f32 = country.bbox[0].into();
        let min_lat: f32 = country.bbox[1].into();
        let max_lon: f32 = country.bbox[2].into();
        let max_lat: f32 = country.bbox[3].into();
        if lon >= min_lon && lon <= max_lon && lat >= min_lat && lat <= max_lat {
            filtered.push(id);
        }
    }

    filtered
}
