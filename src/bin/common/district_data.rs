use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

// -- Types ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoLanguage {
    pub code: String,
    pub name: String,
    pub usage_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictProfile {
    pub district_code: String,
    pub district_name: String,
    pub major_religion: String,
    pub languages: Vec<GeoLanguage>,
}

// -- Public API ----------------------------------------------------------

pub fn load_district_profiles(path: &Path) -> io::Result<HashMap<String, DistrictProfile>> {
    let raw = fs::read_to_string(path)?;
    let mut map: HashMap<String, DistrictProfile> = HashMap::new();

    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Header check only for first row
        if i == 0 && line.eq_ignore_ascii_case(CSV_HEADER) {
            continue;
        }

        let mut parts = line.splitn(6, ',');

        let lang_code = match parts.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        let lang_name = match parts.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        let district_code = match parts.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        let district_name = match parts.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        let usage_type = match parts.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        let major_religion = match parts.next() {
            Some(v) => v.trim(),
            None => continue,
        };

        let entry = map
            .entry(district_code.to_owned())
            .or_insert_with(|| DistrictProfile {
                district_code: district_code.to_owned(),
                district_name: district_name.to_owned(),
                major_religion: major_religion.to_owned(),
                languages: Vec::new(),
            });

        // Only fill religion once if empty
        if entry.major_religion.is_empty() && !major_religion.is_empty() {
            entry.major_religion = major_religion.to_owned();
        }

        entry.languages.push(GeoLanguage {
            code: lang_code.to_owned(),
            name: lang_name.to_owned(),
            usage_type: usage_type.to_owned(),
        });
    }

    // Sort without allocating (no clone)
    for profile in map.values_mut() {
        profile.languages.sort_by(|a, b| {
            usage_rank(&a.usage_type)
                .cmp(&usage_rank(&b.usage_type))
                .then_with(|| a.name.cmp(&b.name))
        });
    }

    Ok(map)
}

pub fn find_district_profile<'a>(
    profiles: &'a HashMap<String, DistrictProfile>,
    district_code: &str,
    district_name: &str,
) -> Option<&'a DistrictProfile> {
    if let Some(p) = profiles.get(district_code) {
        return Some(p);
    }

    profiles
        .values()
        .find(|p| eq_ignore_ascii_trim(&p.district_name, district_name))
}

// -- Constants -----------------------------------------------------------

const CSV_HEADER: &str =
    "lang_code,language_name,district_uni_code,district_name,usage_type,major_religion";

// -- Helpers -------------------------------------------------------------

#[inline]
fn usage_rank(value: &str) -> u8 {
    match value {
        v if v.eq_ignore_ascii_case("primary") => 0,
        v if v.eq_ignore_ascii_case("major") => 1,
        v if v.eq_ignore_ascii_case("administrative") => 2,
        v if v.eq_ignore_ascii_case("minor") => 3,
        _ => 4,
    }
}

#[inline]
fn eq_ignore_ascii_trim(a: &str, b: &str) -> bool {
    trim_ascii(a).eq_ignore_ascii_case(trim_ascii(b))
}

#[inline]
fn trim_ascii(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut end = bytes.len();

    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }

    &s[start..end]
}
