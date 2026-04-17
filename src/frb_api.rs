use crate::{init_path, reverse_geocoding, search};

#[derive(Debug, Clone)]
pub struct FrbRegion {
    pub name: String,
    pub code: String,
}

#[derive(Debug, Clone)]
pub struct FrbCity {
    pub geoname_id: u32,
    pub name: String,
    pub country_name: String,
    pub country_code: String,
    pub latitude: f64,
    pub longitude: f64,
    pub admin1_name: Option<String>,
    pub admin2_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FrbSubdistrict {
    pub subdistrict: FrbRegion,
    pub district: FrbRegion,
    pub state: FrbRegion,
}

#[derive(Debug, Clone)]
pub struct FrbInitResult {
    pub ok: bool,
    pub asset_dir: String,
}

#[derive(Debug, Clone)]
pub struct FrbSearchResult {
    pub cities: Vec<FrbCity>,
    pub subdistricts: Vec<FrbSubdistrict>,
}

#[derive(Debug, Clone)]
pub struct FrbReverseGeocodeResult {
    pub country: FrbRegion,
    pub state: Option<FrbRegion>,
    pub district: Option<FrbRegion>,
    pub subdistrict: Option<FrbRegion>,
    pub nearest_city: FrbCity,
}

pub fn frb_health_check() -> String {
    "geo_engine_ok".to_string()
}

pub fn frb_init(asset_dir: String, verify_checksum: bool) -> Result<FrbInitResult, String> {
    let initialized =
        init_path(asset_dir.clone(), verify_checksum).map_err(|err| err.to_string())?;
    Ok(FrbInitResult {
        ok: initialized,
        asset_dir,
    })
}

pub fn frb_search(query: String) -> Result<FrbSearchResult, String> {
    let result = search(&query).map_err(|err| err.to_string())?;

    let cities = result
        .cities
        .into_iter()
        .map(|city| FrbCity {
            geoname_id: city.geoname_id,
            name: city.name,
            country_name: city.country_name,
            country_code: city.country_code,
            latitude: city.latitude as f64,
            longitude: city.longitude as f64,
            admin1_name: city.admin1_name,
            admin2_name: city.admin2_name,
        })
        .collect();

    let subdistricts = result
        .subdistricts
        .into_iter()
        .map(|subdistrict| FrbSubdistrict {
            subdistrict: FrbRegion {
                name: subdistrict.subdistrict.name,
                code: subdistrict.subdistrict.iso2,
            },
            district: FrbRegion {
                name: subdistrict.district.name,
                code: subdistrict.district.iso2,
            },
            state: FrbRegion {
                name: subdistrict.state.name,
                code: subdistrict.state.iso2,
            },
        })
        .collect();

    Ok(FrbSearchResult {
        cities,
        subdistricts,
    })
}

pub fn frb_reverse_geocode(lat: f64, lon: f64) -> Result<FrbReverseGeocodeResult, String> {
    let result = reverse_geocoding(lat as f32, lon as f32).map_err(|err| err.to_string())?;

    let map_region = |region: crate::engine::api::Region| FrbRegion {
        name: region.name,
        code: region.iso2,
    };

    Ok(FrbReverseGeocodeResult {
        country: map_region(result.country),
        state: result.state.map(map_region),
        district: result.district.map(map_region),
        subdistrict: result.subdistrict.map(map_region),
        nearest_city: FrbCity {
            geoname_id: result.city.geoname_id,
            name: result.city.name,
            country_name: result.city.country_name,
            country_code: result.city.country_code,
            latitude: result.city.latitude as f64,
            longitude: result.city.longitude as f64,
            admin1_name: result.city.admin1_name,
            admin2_name: result.city.admin2_name,
        },
    })
}
