use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use rkyv::util::AlignedVec;
use rkyv::Archived;

use super::error::GeoEngineError;
use super::model::{Country, GeoDB};

pub struct GeoEngine {
    storage: Storage,
}

enum Storage {
    Mmap(Mmap),
    Owned(AlignedVec),
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

        if is_zstd(&mmap[..]) {
            let decoded = zstd::stream::decode_all(&mmap[..]).map_err(|source| {
                GeoEngineError::DatabaseMap {
                    path: PathBuf::from(path_ref),
                    source,
                }
            })?;
            let mut aligned = AlignedVec::with_capacity(decoded.len());
            aligned.extend_from_slice(&decoded);
            return Ok(Self {
                storage: Storage::Owned(aligned),
            });
        }

        Ok(Self {
            storage: Storage::Mmap(mmap),
        })
    }

    pub fn countries(&self) -> &Archived<Vec<Country>> {
        let bytes: &[u8] = match &self.storage {
            Storage::Mmap(mmap) => &mmap[..],
            Storage::Owned(bytes) => bytes,
        };
        let db: &Archived<GeoDB> = rkyv::access::<Archived<GeoDB>, rkyv::rancor::Error>(bytes)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(bytes) });
        &db.countries
    }
}

fn is_zstd(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == [0x28, 0xB5, 0x2F, 0xFD]
}
