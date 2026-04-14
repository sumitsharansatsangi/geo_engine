use rkyv::{Archive, Deserialize, Serialize};

use super::district_data::DistrictProfile;

pub const SUBDISTRICT_FIELD_SEPARATOR: &str = "||";
pub const LANGUAGE_ENTRY_SEPARATOR: &str = "##";
pub const LANGUAGE_COMPONENT_SEPARATOR: &str = "~~";

#[derive(Archive, Serialize, Deserialize, Debug)]
pub struct GeoDB {
    pub countries: Vec<Country>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct Country {
    pub name: String,
    pub iso2: [u8; 2],
    pub bbox: [f32; 4],
    pub polygons: Vec<Vec<(f32, f32)>>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SubdistrictFeature {
    pub name: String,
    pub code: [u8; 2],
    pub polygons: Vec<Vec<(f32, f32)>>,
    pub bbox: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct SubdistrictPayload {
    pub subdistrict_name: String,
    pub district_name: String,
    pub state_name: String,
    pub subdistrict_code: String,
    pub district_code: String,
    pub state_code: String,
}

pub fn parse_subdistrict_payload(raw: &str) -> Option<SubdistrictPayload> {
    let parts: Vec<&str> = raw.split(SUBDISTRICT_FIELD_SEPARATOR).collect();
    if parts.len() < 6 {
        return None;
    }

    Some(SubdistrictPayload {
        subdistrict_name: parts[0].trim().to_string(),
        district_name: parts[1].trim().to_string(),
        state_name: parts[2].trim().to_string(),
        subdistrict_code: parts[3].trim().to_string(),
        district_code: parts[4].trim().to_string(),
        state_code: parts[5].trim().to_string(),
    })
}

pub fn encode_subdistrict_payload(
    subdistrict: &str,
    district: &str,
    state: &str,
    subdistrict_code: &str,
    district_code: &str,
    state_code: &str,
    demographics: Option<&DistrictProfile>,
) -> String {
    let mut fields = vec![
        sanitize_field(subdistrict),
        sanitize_field(district),
        sanitize_field(state),
        sanitize_field(subdistrict_code),
        sanitize_field(district_code),
        sanitize_field(state_code),
    ];

    if let Some(profile) = demographics {
        fields.push(sanitize_field(&profile.district_code));
        fields.push(sanitize_field(&profile.major_religion));
        fields.push(encode_languages(profile));
    }

    fields.join(SUBDISTRICT_FIELD_SEPARATOR)
}

pub fn short_code(name: &str) -> [u8; 2] {
    let mut code = [b' '; 2];
    let mut chars = name
        .bytes()
        .filter(|b| b.is_ascii_alphabetic())
        .map(|b| b.to_ascii_uppercase());

    if let Some(first) = chars.next() {
        code[0] = first;
    }
    if let Some(second) = chars.next() {
        code[1] = second;
    }

    code
}

pub fn short_code_str(name: &str) -> String {
    String::from_utf8_lossy(&short_code(name))
        .trim()
        .to_string()
}

pub fn sanitize_field(value: &str) -> String {
    value
        .replace(SUBDISTRICT_FIELD_SEPARATOR, "|")
        .replace(LANGUAGE_ENTRY_SEPARATOR, "#")
        .replace(LANGUAGE_COMPONENT_SEPARATOR, "~")
}

pub fn encode_languages(profile: &DistrictProfile) -> String {
    profile
        .languages
        .iter()
        .map(|language| {
            format!(
                "{}{}{}{}{}",
                sanitize_field(&language.name),
                LANGUAGE_COMPONENT_SEPARATOR,
                sanitize_field(&language.usage_type),
                LANGUAGE_COMPONENT_SEPARATOR,
                sanitize_field(&language.code)
            )
        })
        .collect::<Vec<String>>()
        .join(LANGUAGE_ENTRY_SEPARATOR)
}

pub fn is_zstd(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == [0x28, 0xB5, 0x2F, 0xFD]
}
