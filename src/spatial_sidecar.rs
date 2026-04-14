use rkyv::{Archive, Deserialize, Serialize};

pub const SPATIAL_BACKEND_H3: u8 = 1;
pub const SPATIAL_BACKEND_S2: u8 = 2;

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct CellBucket {
    pub cell: u64,
    pub country_ids: Vec<u32>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SpatialSidecarFile {
    pub version: u8,
    pub backend: u8,
    pub level: u8,
    pub cells: Vec<CellBucket>,
    pub polygon_country_ids: Vec<u32>,
    pub polygon_ring_ids: Vec<u32>,
    pub min_lon: Vec<f32>,
    pub min_lat: Vec<f32>,
    pub max_lon: Vec<f32>,
    pub max_lat: Vec<f32>,
}
