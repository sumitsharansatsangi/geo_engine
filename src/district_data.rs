use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictLanguage {
    pub code: String,
    pub name: String,
    pub usage_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictProfile {
    pub district_code: String,
    pub district_name: String,
    pub major_religion: String,
    pub languages: Vec<DistrictLanguage>,
}

pub fn load_district_profiles(path: &Path) -> Result<HashMap<String, DistrictProfile>, io::Error> {
    let raw = fs::read_to_string(path)?;
    let mut profiles = HashMap::new();

    for (index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if index == 0 && line.eq_ignore_ascii_case(HEADER) {
            continue;
        }

        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() != 6 {
            continue;
        }

        let district_code = parts[2].to_string();
        let district_name = parts[3].to_string();
        let major_religion = parts[5].to_string();

        let profile = profiles
            .entry(district_code.clone())
            .or_insert_with(|| DistrictProfile {
                district_code: district_code.clone(),
                district_name: district_name.clone(),
                major_religion: major_religion.clone(),
                languages: Vec::new(),
            });

        if profile.major_religion.is_empty() && !major_religion.is_empty() {
            profile.major_religion = major_religion;
        }

        profile.languages.push(DistrictLanguage {
            code: parts[0].to_string(),
            name: parts[1].to_string(),
            usage_type: parts[4].to_string(),
        });
    }

    for profile in profiles.values_mut() {
        profile.languages.sort_by(|left, right| {
            usage_rank(&left.usage_type)
                .cmp(&usage_rank(&right.usage_type))
                .then_with(|| left.name.cmp(&right.name))
        });
    }

    Ok(profiles)
}

pub fn find_district_profile<'a>(
    profiles: &'a HashMap<String, DistrictProfile>,
    district_code: &str,
    district_name: &str,
) -> Option<&'a DistrictProfile> {
    if let Some(profile) = profiles.get(district_code) {
        return Some(profile);
    }

    let normalized_target = normalize_key(district_name);
    profiles
        .values()
        .find(|profile| normalize_key(&profile.district_name) == normalized_target)
}

const HEADER: &str =
    "lang_code,language_name,district_uni_code,district_name,usage_type,major_religion";

fn usage_rank(value: &str) -> u8 {
    match value.to_ascii_lowercase().as_str() {
        "primary" => 0,
        "major" => 1,
        "administrative" => 2,
        "minor" => 3,
        _ => 4,
    }
}

fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
