#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug)]
pub struct SubdistrictMeta {
    pub strings: Vec<String>,
    pub entries: Vec<SubdistrictMetaEntry>,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug)]
pub struct SubdistrictMetaEntry {
    pub subdistrict_name_id: u32,
    pub district_name_id: u32,
    pub state_name_id: u32,
    pub subdistrict_code_id: u32,
    pub district_code_id: u32,
    pub state_code_id: u32,
    pub district_uni_code_id: Option<u32>,
    pub major_religion_id: Option<u32>,
    pub languages_blob_id: Option<u32>,
}
