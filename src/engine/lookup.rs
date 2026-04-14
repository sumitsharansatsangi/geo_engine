use rkyv::Archived;

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
    for country_id in index.candidates(lat, lon) {
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
