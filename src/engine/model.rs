use rkyv::{Archive, Deserialize, Serialize};

#[derive(Archive, Serialize, Deserialize, Debug)]
pub struct GeoDB {
    pub countries: Vec<Country>,
}

/// A country polygon record stored in the rkyv database.
///
/// `bbox`    – `[min_lon, min_lat, max_lon, max_lat]`  
/// `polygons` – each inner `Vec` is one ring; coordinates are `(lon, lat)`.
#[derive(Archive, Serialize, Deserialize, Debug)]
pub struct Country {
    pub name: String,
    pub iso2: [u8; 2],
    pub bbox: [f32; 4],
    pub polygons: Vec<Vec<(f32, f32)>>,
}
