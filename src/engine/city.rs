use unicode_normalization::UnicodeNormalization;


// ----------- TYPES -----------

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug)]
pub struct City {
    pub name: String,
    pub ascii: String,
    pub alternates: Vec<String>,
    pub lat: f32,
    pub lon: f32,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone)]
pub struct CityPoint {
    pub id: u32,
    pub lat: f32,
    pub lon: f32,
}

// ----------- NORMALIZE -----------

pub fn normalize(s: &str) -> String {
    s.nfkd()
     .filter(|c| !c.is_ascii_punctuation())
     .collect::<String>()
     .to_lowercase()
}

