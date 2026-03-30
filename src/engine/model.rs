use rkyv::{Archive, Deserialize, Serialize};

#[derive(Archive, Serialize, Deserialize, Debug)]
pub struct GeoDB {
    pub countries: Vec<Country>,
}

#[derive(Archive, Serialize, Deserialize, Debug)]
pub struct Country {
    pub name: String,

    pub iso2: [u8; 2],

    // min_lon, min_lat, max_lon, max_lat
    pub bbox: [f32; 4],

    pub polygons: Vec<Vec<(f32, f32)>>,
}
