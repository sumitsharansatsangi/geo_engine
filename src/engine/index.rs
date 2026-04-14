use rkyv::Archived;
use rstar::{AABB, RTree, RTreeObject};

use crate::engine::model::Country;

pub struct CountryBBox {
    id: u32,
    envelope: AABB<[f32; 2]>,
}

impl RTreeObject for CountryBBox {
    type Envelope = AABB<[f32; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

pub struct PolygonBBox {
    country_id: u32,
    ring_id: u32,
    envelope: AABB<[f32; 2]>,
}

impl RTreeObject for PolygonBBox {
    type Envelope = AABB<[f32; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

pub struct SpatialIndex {
    country_tree: RTree<CountryBBox>,
    polygon_tree: RTree<PolygonBBox>,
}

impl SpatialIndex {
    pub fn build(countries: &Archived<Vec<Country>>) -> Self {
        let country_items = countries
            .iter()
            .enumerate()
            .map(|(idx, country)| {
                let bbox = country.bbox;
                let min = [bbox[0].into(), bbox[1].into()];
                let max = [bbox[2].into(), bbox[3].into()];
                CountryBBox {
                    id: idx as u32,
                    envelope: AABB::from_corners(min, max),
                }
            })
            .collect();

        let mut polygon_items = Vec::new();
        for (country_idx, country) in countries.iter().enumerate() {
            for (ring_idx, ring) in country.polygons.iter().enumerate() {
                let mut min_lon = f32::INFINITY;
                let mut min_lat = f32::INFINITY;
                let mut max_lon = f32::NEG_INFINITY;
                let mut max_lat = f32::NEG_INFINITY;

                for point in ring.iter() {
                    let lon: f32 = point.0.into();
                    let lat: f32 = point.1.into();
                    min_lon = min_lon.min(lon);
                    min_lat = min_lat.min(lat);
                    max_lon = max_lon.max(lon);
                    max_lat = max_lat.max(lat);
                }

                if min_lon.is_finite()
                    && min_lat.is_finite()
                    && max_lon.is_finite()
                    && max_lat.is_finite()
                {
                    polygon_items.push(PolygonBBox {
                        country_id: country_idx as u32,
                        ring_id: ring_idx as u32,
                        envelope: AABB::from_corners([min_lon, min_lat], [max_lon, max_lat]),
                    });
                }
            }
        }

        let country_tree = RTree::bulk_load(country_items);
        let polygon_tree = RTree::bulk_load(polygon_items);

        Self {
            country_tree,
            polygon_tree,
        }
    }

    pub fn candidates(&self, lat: f32, lon: f32) -> impl Iterator<Item = u32> + '_ {
        let envelope = AABB::from_point([lon, lat]);

        self.country_tree
            .locate_in_envelope_intersecting(&envelope)
            .map(|c| c.id)
    }

    pub fn polygon_candidates(&self, lat: f32, lon: f32) -> impl Iterator<Item = (u32, u32)> + '_ {
        let envelope = AABB::from_point([lon, lat]);

        self.polygon_tree
            .locate_in_envelope_intersecting(&envelope)
            .map(|polygon| (polygon.country_id, polygon.ring_id))
    }
}
