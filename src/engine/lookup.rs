use rkyv::Archived;

use super::error::GeoEngineError;
use super::index::SpatialIndex;
use super::model::Country;
use super::polygon::point_in_ring;

pub fn find_country<'a>(
    lat: f32,
    lon: f32,
    index: &'a SpatialIndex,
) -> Result<&'a Archived<Country>, GeoEngineError> {
    let candidates = index.candidates(lat, lon);

    for country in candidates {
        for ring in country.polygons.iter() {
            if point_in_ring(lat, lon, ring) {
                return Ok(country);
            }
        }
    }

    Err(GeoEngineError::CountryNotFound { lat, lon })
}
