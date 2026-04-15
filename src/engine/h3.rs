use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use h3o::{LatLng, Resolution};
use rkyv::{Archive, Deserialize, Serialize};

use super::error::GeoEngineError;

#[derive(Archive, Serialize, Deserialize, Debug)]
pub struct H3IndexFile {
    pub resolution: u8,
    pub cells: Vec<H3CellEntry>,
}

#[derive(Archive, Serialize, Deserialize, Debug)]
pub struct H3CellEntry {
    pub cell: u64,
    pub polygon_ids: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct H3RuntimeIndex {
    resolution: u8,
    buckets: HashMap<u64, Vec<u32>>,
}

impl H3RuntimeIndex {
    pub fn from_file(path: &Path) -> Result<Self, GeoEngineError> {
        let bytes = fs::read(path).map_err(|source| GeoEngineError::DatabaseOpen {
            path: path.to_path_buf(),
            source,
        })?;

        let archived: &rkyv::Archived<H3IndexFile> =
            rkyv::access::<rkyv::Archived<H3IndexFile>, rkyv::rancor::Error>(&bytes).map_err(
                |source| {
                    operation_failed(
                        "h3.from_file.decode_sidecar",
                        GeoEngineError::DatabaseMap {
                            path: path.to_path_buf(),
                            source: std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("failed to decode h3 sidecar: {source}"),
                            ),
                        },
                    )
                },
            )?;

        let mut buckets = HashMap::with_capacity(archived.cells.len());
        for entry in archived.cells.iter() {
            let mut ids: Vec<u32> = entry.polygon_ids.iter().map(Into::into).collect();
            ids.sort_unstable();
            ids.dedup();
            buckets.insert(entry.cell.into(), ids);
        }

        Ok(Self {
            resolution: archived.resolution.into(),
            buckets,
        })
    }

    pub fn candidate_ids(&self, lat: f32, lon: f32) -> Option<&[u32]> {
        let cell = point_to_cell(lat, lon, self.resolution)?;
        self.buckets.get(&cell).map(Vec::as_slice)
    }
}

pub fn default_sidecar_path(db_path: &Path) -> PathBuf {
    db_path.with_extension("h3")
}

pub fn point_to_cell(lat: f32, lon: f32, resolution: u8) -> Option<u64> {
    let latlng = LatLng::new(lat as f64, lon as f64).ok()?;
    let res = Resolution::try_from(resolution).ok()?;
    Some(u64::from(latlng.to_cell(res)))
}

pub fn merge_candidate_ids(h3: Option<&[u32]>, rtree: impl Iterator<Item = u32>) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut seen = HashSet::new();

    if let Some(h3_ids) = h3 {
        for &id in h3_ids {
            if seen.insert(id) {
                ids.push(id);
            }
        }
    }

    for id in rtree {
        if seen.insert(id) {
            ids.push(id);
        }
    }

    ids
}

fn operation_failed(operation: &'static str, source: GeoEngineError) -> GeoEngineError {
    GeoEngineError::OperationFailed {
        operation,
        source: Box::new(source),
    }
}
