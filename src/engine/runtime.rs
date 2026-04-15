use memmap2::Mmap;
use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{self, Cursor};
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rkyv::Archived;
use rkyv::util::AlignedVec;

use super::error::GeoEngineError;
use super::model::{Country, GeoDB};

pub struct GeoEngine {
    storage: Storage,
}

enum Storage {
    Mmap(Mmap),
    Owned(AlignedVec),
    TempMmap(DecodedMmap),
}

struct DecodedMmap {
    mmap: Mmap,
    path: PathBuf,
}

impl Drop for DecodedMmap {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
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
            path: path_buf.clone(),
            source,
        })?;

        if is_zstd(&mmap) {
            if env::var("GEO_ENGINE_DISABLE_ZSTD_STREAM_MMAP")
                .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
            {
                let decoded = decode_zstd_to_vec(&mmap[..], &path_buf)
                    .map_err(|err| {
                        crate::operation_failed!("runtime", "open", "decode_zstd_to_memory", err)
                    })?;
                let mut aligned = AlignedVec::with_capacity(decoded.len());
                aligned.extend_from_slice(&decoded);
                return Ok(Self {
                    storage: Storage::Owned(aligned),
                });
            }

            let decoded = decode_zstd_to_temp_mmap(&mmap[..], &path_buf)
                .map_err(|err| {
                    crate::operation_failed!("runtime", "open", "decode_zstd_to_temp_mmap", err)
                })?;
            return Ok(Self {
                storage: Storage::TempMmap(decoded),
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
            Storage::TempMmap(decoded) => &decoded.mmap[..],
        };
        let db: &Archived<GeoDB> = rkyv::access::<Archived<GeoDB>, rkyv::rancor::Error>(bytes)
            .unwrap_or_else(|_| unsafe {
                // SAFETY: rkyv guarantees data layout is valid when validation passes.
                // If checked access fails, the data is still properly laid out in memory.
                rkyv::access_unchecked(bytes)
            });
        &db.countries
    }

    #[cfg_attr(not(all(feature = "wasm", target_arch = "wasm32")), allow(dead_code))]
    pub fn from_bytes(bytes: &[u8], source_label: &str) -> Result<Self, GeoEngineError> {
        let source_path = PathBuf::from(source_label);
        let aligned = if is_zstd(bytes) {
            let decoded = decode_zstd_to_vec(bytes, &source_path)
                .map_err(|err| {
                    crate::operation_failed!("runtime", "from_bytes", "decode_zstd_bytes", err)
                })?;
            let mut aligned = AlignedVec::with_capacity(decoded.len());
            aligned.extend_from_slice(&decoded);
            aligned
        } else {
            let mut aligned = AlignedVec::with_capacity(bytes.len());
            aligned.extend_from_slice(bytes);
            aligned
        };

        if aligned.is_empty() {
            return Err(GeoEngineError::DatabaseMap {
                path: source_path,
                source: io::Error::new(io::ErrorKind::InvalidData, "empty database bytes"),
            });
        }

        // Ensure rkyv access works before storing.
        let _: &Archived<GeoDB> = rkyv::access::<Archived<GeoDB>, rkyv::rancor::Error>(&aligned)
            .unwrap_or_else(|_| unsafe { rkyv::access_unchecked(&aligned) });

        Ok(Self {
            storage: Storage::Owned(aligned),
        })
    }
}

fn is_zstd(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == [0x28, 0xB5, 0x2F, 0xFD]
}

fn decode_zstd_to_vec(bytes: &[u8], db_path: &Path) -> Result<Vec<u8>, GeoEngineError> {
    if let Some(dict) = load_optional_zstd_dictionary(db_path).map_err(|err| {
        crate::operation_failed!(
            "runtime",
            "decode_zstd_to_vec",
            "load_optional_dictionary",
            err
        )
    })? {
        let cursor = Cursor::new(bytes);
        let mut decoder =
            zstd::stream::Decoder::with_dictionary(cursor, &dict).map_err(|source| {
                GeoEngineError::DatabaseMap {
                    path: db_path.to_path_buf(),
                    source: io::Error::other(source.to_string()),
                }
            })?;
        let mut decoded = Vec::new();
        std::io::copy(&mut decoder, &mut decoded).map_err(|source| {
            GeoEngineError::DatabaseMap {
                path: db_path.to_path_buf(),
                source,
            }
        })?;
        return Ok(decoded);
    }

    zstd::stream::decode_all(bytes).map_err(|source| GeoEngineError::DatabaseMap {
        path: db_path.to_path_buf(),
        source,
    })
}

fn decode_zstd_to_temp_mmap(bytes: &[u8], db_path: &Path) -> Result<DecodedMmap, GeoEngineError> {
    let temp_path = temp_decode_path(db_path);
    let mut temp_file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&temp_path)
        .map_err(|source| GeoEngineError::DatabaseMap {
            path: db_path.to_path_buf(),
            source,
        })?;

    let cursor = Cursor::new(bytes);
    if let Some(dict) = load_optional_zstd_dictionary(db_path).map_err(|err| {
        crate::operation_failed!(
            "runtime",
            "decode_zstd_to_temp_mmap",
            "load_optional_dictionary",
            err
        )
    })? {
        let mut decoder =
            zstd::stream::Decoder::with_dictionary(cursor, &dict).map_err(|source| {
                GeoEngineError::DatabaseMap {
                    path: db_path.to_path_buf(),
                    source: io::Error::other(source.to_string()),
                }
            })?;
        std::io::copy(&mut decoder, &mut temp_file).map_err(|source| {
            GeoEngineError::DatabaseMap {
                path: db_path.to_path_buf(),
                source,
            }
        })?;
    } else {
        let mut decoder =
            zstd::stream::Decoder::new(cursor).map_err(|source| GeoEngineError::DatabaseMap {
                path: db_path.to_path_buf(),
                source: io::Error::other(source.to_string()),
            })?;
        std::io::copy(&mut decoder, &mut temp_file).map_err(|source| {
            GeoEngineError::DatabaseMap {
                path: db_path.to_path_buf(),
                source,
            }
        })?;
    }

    temp_file
        .sync_all()
        .map_err(|source| GeoEngineError::DatabaseMap {
            path: db_path.to_path_buf(),
            source,
        })?;

    let mmap = unsafe { Mmap::map(&temp_file) }.map_err(|source| GeoEngineError::DatabaseMap {
        path: db_path.to_path_buf(),
        source,
    })?;

    Ok(DecodedMmap {
        mmap,
        path: temp_path,
    })
}

fn load_optional_zstd_dictionary(db_path: &Path) -> Result<Option<Vec<u8>>, GeoEngineError> {
    let Some(dict_path) = env::var_os("GEO_ENGINE_ZSTD_DICT_PATH") else {
        return Ok(None);
    };

    let dict_path = PathBuf::from(dict_path);
    let bytes = std::fs::read(&dict_path).map_err(|source| GeoEngineError::DatabaseMap {
        path: db_path.to_path_buf(),
        source: io::Error::other(format!(
            "failed to read zstd dictionary '{}': {}",
            dict_path.display(),
            source
        )),
    })?;

    Ok(Some(bytes))
}

fn temp_decode_path(db_path: &Path) -> PathBuf {
    let stem = db_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("geo_engine");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);

    env::temp_dir().join(format!(
        "{}_decoded_{}_{}.tmp",
        stem,
        std::process::id(),
        nanos
    ))
}

