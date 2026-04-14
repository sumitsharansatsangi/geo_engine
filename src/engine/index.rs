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

pub struct SpatialIndex {
    tree: RTree<CountryBBox>,
}

impl SpatialIndex {
    pub fn build(countries: &Archived<Vec<Country>>) -> Self {
        let items = countries
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

        let tree = RTree::bulk_load(items);

        Self { tree }
    }

    pub fn candidates(&self, lat: f32, lon: f32) -> impl Iterator<Item = u32> + '_ {
        let envelope = AABB::from_point([lon, lat]);

        self.tree
            .locate_in_envelope_intersecting(&envelope)
            .map(|c| c.id)
    }
}
