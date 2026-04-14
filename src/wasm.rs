#![cfg(all(feature = "wasm", target_arch = "wasm32"))]

use wasm_bindgen::prelude::*;

use crate::engine::api::InitializedGeoEngine;

fn to_js_error<E: std::fmt::Display>(err: E) -> JsValue {
    JsValue::from_str(&err.to_string())
}

#[wasm_bindgen]
pub struct WasmGeoEngine {
    inner: InitializedGeoEngine,
}

#[wasm_bindgen]
impl WasmGeoEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(
        country_db: &[u8],
        subdistrict_db: &[u8],
        city_fst: &[u8],
        city_rkyv: &[u8],
    ) -> Result<WasmGeoEngine, JsValue> {
        let inner = InitializedGeoEngine::open_from_bytes(
            country_db,
            Some(subdistrict_db),
            Some(city_fst),
            Some(city_rkyv),
        )
        .map_err(to_js_error)?;

        Ok(Self { inner })
    }

    pub fn reverse_geocoding(&self, lat: f32, lon: f32) -> Result<JsValue, JsValue> {
        let result = self
            .inner
            .reverse_geocoding(lat, lon)
            .map_err(to_js_error)?;
        serde_wasm_bindgen::to_value(&result).map_err(to_js_error)
    }

    pub fn search(&self, query: &str) -> Result<JsValue, JsValue> {
        let result = self
            .inner
            .search_places_by_name(query, None)
            .map_err(to_js_error)?;
        serde_wasm_bindgen::to_value(&result).map_err(to_js_error)
    }

    pub fn reverse_geocoding_batch(&self, coordinates: &[f32]) -> Result<JsValue, JsValue> {
        if !coordinates.len().is_multiple_of(2) {
            return Err(JsValue::from_str(
                "coordinates length must be even: [lat0, lon0, lat1, lon1, ...]",
            ));
        }

        let mut results = Vec::with_capacity(coordinates.len() / 2);
        for pair in coordinates.chunks_exact(2) {
            let lat = pair[0];
            let lon = pair[1];
            results.push(
                self.inner
                    .reverse_geocoding(lat, lon)
                    .map_err(|e| e.to_string()),
            );
        }

        serde_wasm_bindgen::to_value(&results).map_err(to_js_error)
    }
}
