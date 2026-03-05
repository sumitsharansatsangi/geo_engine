use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use rkyv::Archived;

use super::error::GeoEngineError;
use super::model::{Country, GeoDB};

pub struct GeoEngine {
    storage: Storage,
}

enum Storage {
    Mmap(Mmap),
    Static(&'static [u8]),
}

impl GeoEngine {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, GeoEngineError> {
        let path_ref = path.as_ref();
        let path_buf = PathBuf::from(path_ref);
        let file = File::open(path_ref).map_err(|source| GeoEngineError::DatabaseOpen {
            path: path_buf.clone(),
            source,
        })?;

        let mmap = unsafe { Mmap::map(&file) }.map_err(|source| GeoEngineError::DatabaseMap {
            path: path_buf,
            source,
        })?;

        Ok(Self {
            storage: Storage::Mmap(mmap),
        })
    }

    pub fn from_static_bytes(bytes: &'static [u8]) -> Self {
        Self {
            storage: Storage::Static(bytes),
        }
    }

    pub fn countries(&self) -> &Archived<Vec<Country>> {
        let bytes: &[u8] = match &self.storage {
            Storage::Mmap(mmap) => &mmap[..],
            Storage::Static(bytes) => bytes,
        };
        let db: &Archived<GeoDB> = unsafe { rkyv::access_unchecked(bytes) };
        &db.countries
    }
}
