use rkyv::Archived;
use rstar::{AABB, RTree, RTreeObject};

use crate::engine::model::Country;

pub struct CountryBBox<'a> {
    pub country: &'a Archived<Country>,
}

impl<'a> RTreeObject for CountryBBox<'a> {
    type Envelope = AABB<[f32; 2]>;
    fn envelope(&self) -> Self::Envelope {
        let bbox = self.country.bbox;

        let min = [bbox[0].into(), bbox[1].into()];
        let max = [bbox[2].into(), bbox[3].into()];

        AABB::from_corners(min, max)
    }
}

pub struct SpatialIndex<'a> {
    tree: RTree<CountryBBox<'a>>,
}

impl<'a> SpatialIndex<'a> {
    pub fn build(countries: &'a Archived<Vec<Country>>) -> Self {
        let items = countries
            .iter()
            .map(|c| CountryBBox { country: c })
            .collect();

        let tree = RTree::bulk_load(items);

        Self { tree }
    }

    pub fn candidates(&self, lat: f32, lon: f32) -> Vec<&'a Archived<Country>> {
        let point = [lon, lat];
        let envelope = AABB::from_point(point);

        self.tree
            .locate_in_envelope_intersecting(&envelope)
            .map(|c| c.country)
            .collect()
    }
}
