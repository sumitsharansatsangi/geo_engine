use crate::engine::api::InitializedGeoEngine;
use crate::engine::error::GeoEngineError;
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const GEO_DB_NAME: &str = "geo-0.0.1.db";
const SUBDISTRICT_DB_NAME: &str = "subdistrict-0.0.1.db";
const CITY_FST_NAME: &str = "cities-0.0.1.fst";
const CITY_RKYV_NAME: &str = "cities-0.0.1.rkyv";
const CITY_POINTS_NAME: &str = "cities-0.0.1.points";

const GEO_DB_URL: &str =
    "https://github.com/sumitsharansatsangi/geo_engine/releases/download/v0.0.1/geo-0.0.1.db";
const SUBDISTRICT_DB_URL: &str = "https://github.com/sumitsharansatsangi/geo_engine/releases/download/v0.0.1/subdistrict-0.0.1.db";
const CITY_FST_URL: &str =
    "https://github.com/sumitsharansatsangi/geo_engine/releases/download/v0.0.1/cities-0.0.1.fst";
const CITY_RKYV_URL: &str =
    "https://github.com/sumitsharansatsangi/geo_engine/releases/download/v0.0.1/cities-0.0.1.rkyv";
const CITY_POINTS_URL: &str = "https://github.com/sumitsharansatsangi/geo_engine/releases/download/v0.0.1/cities-0.0.1.points";

// SHA-256 checksums for v0.0.1 releases
const GEO_DB_SHA256: &str = "44c2b0887d044135336538f0f67df3d49f2e8b64d04d4b2b3c03fb6d946f7fa0";
const SUBDISTRICT_DB_SHA256: &str =
    "72ce3c7c8e3cfdea2d354172c4d5536044b05e8d2b91a5a2dda72326fb0291aa";
const CITY_FST_SHA256: &str = "8bb3a2f202db0864537e8ebd3bdc31c229218ca06a8ca787df5b7d7112a51995";
const CITY_RKYV_SHA256: &str = "7da471653c444d3b1b16070a33819653f04f9f100a1065b951e89b86d6e1a6fb";
const CITY_POINTS_SHA256: &str = "ac5836cf4a7a0bd93a96638830bcba546c61eec59b13ebf8317bfafdf3d0b46e";

/// Configuration for asset initialization
#[derive(Debug, Clone)]
pub struct InitConfig {
    /// Path where assets should be downloaded/stored
    pub asset_dir: PathBuf,
    /// Whether to verify checksums of existing files
    pub verify_checksum: bool,
}

pub struct CityAssetPaths {
    pub fst_path: PathBuf,
    pub rkyv_path: PathBuf,
    pub points_path: PathBuf,
}

pub struct AllAssetPaths {
    pub geo_db_path: PathBuf,
    pub subdistrict_db_path: PathBuf,
    pub city_fst_path: PathBuf,
    pub city_rkyv_path: PathBuf,
    pub city_points_path: PathBuf,
}

/// Initialize all required assets in the provided directory.
///
/// This method always verifies SHA-256 checksums of existing files.
/// Missing files are downloaded, and invalid/incomplete files are redownloaded.
pub fn init_all_assets(asset_dir: &Path) -> Result<AllAssetPaths, GeoEngineError> {
    let config = InitConfig {
        asset_dir: asset_dir.to_path_buf(),
        verify_checksum: true,
    };
    init_all_assets_with_config(&config)
}

pub fn init_all_assets_with_config(config: &InitConfig) -> Result<AllAssetPaths, GeoEngineError> {
    fs::create_dir_all(&config.asset_dir).map_err(|source| {
        GeoEngineError::CacheDirectoryUnavailable {
            path: config.asset_dir.clone(),
            source,
        }
    })?;

    let geo_db_path = ensure_asset(
        &config.asset_dir,
        GEO_DB_NAME,
        GEO_DB_URL,
        GEO_DB_SHA256,
        config.verify_checksum,
    )?;
    let subdistrict_db_path = ensure_asset(
        &config.asset_dir,
        SUBDISTRICT_DB_NAME,
        SUBDISTRICT_DB_URL,
        SUBDISTRICT_DB_SHA256,
        config.verify_checksum,
    )?;
    let city_fst_path = ensure_asset(
        &config.asset_dir,
        CITY_FST_NAME,
        CITY_FST_URL,
        CITY_FST_SHA256,
        config.verify_checksum,
    )?;
    let city_rkyv_path = ensure_asset(
        &config.asset_dir,
        CITY_RKYV_NAME,
        CITY_RKYV_URL,
        CITY_RKYV_SHA256,
        config.verify_checksum,
    )?;
    let city_points_path = ensure_asset(
        &config.asset_dir,
        CITY_POINTS_NAME,
        CITY_POINTS_URL,
        CITY_POINTS_SHA256,
        config.verify_checksum,
    )?;

    Ok(AllAssetPaths {
        geo_db_path,
        subdistrict_db_path,
        city_fst_path,
        city_rkyv_path,
        city_points_path,
    })
}

/// Start a background refresh for all assets in the provided directory.
///
/// Existing files stay usable while downloads happen. Each file is replaced only
/// after a successful checksum-verified download.
pub fn init_all_assets_in_background(
    asset_dir: &Path,
) -> Result<thread::JoinHandle<Result<AllAssetPaths, GeoEngineError>>, GeoEngineError> {
    let config = InitConfig {
        asset_dir: asset_dir.to_path_buf(),
        verify_checksum: true,
    };
    init_all_assets_in_background_with_config(&config)
}

pub fn init_all_assets_in_background_with_config(
    config: &InitConfig,
) -> Result<thread::JoinHandle<Result<AllAssetPaths, GeoEngineError>>, GeoEngineError> {
    fs::create_dir_all(&config.asset_dir).map_err(|source| {
        GeoEngineError::CacheDirectoryUnavailable {
            path: config.asset_dir.clone(),
            source,
        }
    })?;

    let config_owned = config.clone();
    Ok(thread::spawn(move || {
        init_all_assets_with_config(&config_owned)
    }))
}

/// Fire-and-forget background refresh for all assets.
///
/// This starts a background thread and immediately returns. Existing files
/// remain available while refresh runs; files are atomically replaced only
/// after successful download and checksum verification.
pub fn refresh_all_assets_in_background(asset_dir: &Path) -> Result<(), GeoEngineError> {
    let config = InitConfig {
        asset_dir: asset_dir.to_path_buf(),
        verify_checksum: true,
    };
    refresh_all_assets_in_background_with_config(&config)
}

pub fn refresh_all_assets_in_background_with_config(
    config: &InitConfig,
) -> Result<(), GeoEngineError> {
    let asset_dir = config.asset_dir.clone();
    refresh_all_assets_in_background_with_callback_config(config, move |result| match result {
        Ok(_) => {
            eprintln!(
                "geo_engine: background asset refresh completed in '{}'",
                asset_dir.display()
            );
        }
        Err(err) => {
            eprintln!("geo_engine: background asset refresh failed: {err}");
        }
    })
}

/// Start a background refresh and invoke a callback when it finishes.
///
/// The callback receives the verified asset paths on success or the error on failure.
/// Existing files remain available while refresh runs, and are replaced atomically
/// only after the new asset has been downloaded and checksum-verified.
pub fn refresh_all_assets_in_background_with_callback<F>(
    asset_dir: &Path,
    on_complete: F,
) -> Result<(), GeoEngineError>
where
    F: FnOnce(Result<AllAssetPaths, GeoEngineError>) + Send + 'static,
{
    let config = InitConfig {
        asset_dir: asset_dir.to_path_buf(),
        verify_checksum: true,
    };
    refresh_all_assets_in_background_with_callback_config(&config, on_complete)
}

pub fn refresh_all_assets_in_background_with_callback_config<F>(
    config: &InitConfig,
    on_complete: F,
) -> Result<(), GeoEngineError>
where
    F: FnOnce(Result<AllAssetPaths, GeoEngineError>) + Send + 'static,
{
    fs::create_dir_all(&config.asset_dir).map_err(|source| {
        GeoEngineError::CacheDirectoryUnavailable {
            path: config.asset_dir.clone(),
            source,
        }
    })?;

    let config_owned = config.clone();
    let _ = thread::spawn(move || {
        let result = init_all_assets_with_config(&config_owned);
        on_complete(result);
    });

    Ok(())
}

/// Refresh all assets in the background and reopen the geo engine on success.
///
/// The callback receives a freshly opened `InitializedGeoEngine` after the
/// refreshed geo and subdistrict databases are verified and written atomically.
pub fn refresh_and_reopen_engine_in_background<F>(
    asset_dir: &Path,
    on_complete: F,
) -> Result<(), GeoEngineError>
where
    F: FnOnce(Result<InitializedGeoEngine, GeoEngineError>) + Send + 'static,
{
    let config = InitConfig {
        asset_dir: asset_dir.to_path_buf(),
        verify_checksum: true,
    };
    refresh_and_reopen_engine_in_background_with_config(&config, on_complete)
}

pub fn refresh_and_reopen_engine_in_background_with_config<F>(
    config: &InitConfig,
    on_complete: F,
) -> Result<(), GeoEngineError>
where
    F: FnOnce(Result<InitializedGeoEngine, GeoEngineError>) + Send + 'static,
{
    fs::create_dir_all(&config.asset_dir).map_err(|source| {
        GeoEngineError::CacheDirectoryUnavailable {
            path: config.asset_dir.clone(),
            source,
        }
    })?;

    let config_owned = config.clone();
    let _ = thread::spawn(move || {
        let result = init_all_assets_with_config(&config_owned).and_then(|paths| {
            InitializedGeoEngine::open(&paths.geo_db_path, Some(&paths.subdistrict_db_path))
        });
        on_complete(result);
    });

    Ok(())
}

/// Initialize geo engine with default configuration (uses cache directory, checksums disabled)
pub fn init_geo_engine() -> Result<InitializedGeoEngine, GeoEngineError> {
    let cache_dir = cache_dir()?;
    let config = InitConfig {
        asset_dir: cache_dir,
        verify_checksum: false,
    };
    init_geo_engine_with_config(&config)
}

/// Initialize geo engine with custom configuration
pub fn init_geo_engine_with_config(
    config: &InitConfig,
) -> Result<InitializedGeoEngine, GeoEngineError> {
    fs::create_dir_all(&config.asset_dir).map_err(|source| {
        GeoEngineError::CacheDirectoryUnavailable {
            path: config.asset_dir.clone(),
            source,
        }
    })?;

    let geo_db_path = ensure_asset(
        &config.asset_dir,
        GEO_DB_NAME,
        GEO_DB_URL,
        GEO_DB_SHA256,
        config.verify_checksum,
    )?;
    let subdistrict_db_path = ensure_asset(
        &config.asset_dir,
        SUBDISTRICT_DB_NAME,
        SUBDISTRICT_DB_URL,
        SUBDISTRICT_DB_SHA256,
        config.verify_checksum,
    )?;

    InitializedGeoEngine::open(&geo_db_path, Some(&subdistrict_db_path))
}

/// Initialize city assets with default configuration (uses cache directory, checksums disabled)
pub fn init_city_assets() -> Result<CityAssetPaths, GeoEngineError> {
    let cache_dir = cache_dir()?;
    let config = InitConfig {
        asset_dir: cache_dir,
        verify_checksum: false,
    };
    init_city_assets_with_config(&config)
}

/// Initialize city assets with custom configuration
pub fn init_city_assets_with_config(config: &InitConfig) -> Result<CityAssetPaths, GeoEngineError> {
    fs::create_dir_all(&config.asset_dir).map_err(|source| {
        GeoEngineError::CacheDirectoryUnavailable {
            path: config.asset_dir.clone(),
            source,
        }
    })?;

    let fst_path = ensure_asset(
        &config.asset_dir,
        CITY_FST_NAME,
        CITY_FST_URL,
        CITY_FST_SHA256,
        config.verify_checksum,
    )?;
    let rkyv_path = ensure_asset(
        &config.asset_dir,
        CITY_RKYV_NAME,
        CITY_RKYV_URL,
        CITY_RKYV_SHA256,
        config.verify_checksum,
    )?;
    let points_path = ensure_asset(
        &config.asset_dir,
        CITY_POINTS_NAME,
        CITY_POINTS_URL,
        CITY_POINTS_SHA256,
        config.verify_checksum,
    )?;

    Ok(CityAssetPaths {
        fst_path,
        rkyv_path,
        points_path,
    })
}

fn ensure_asset(
    asset_dir: &Path,
    asset_name: &str,
    url: &str,
    expected_sha256: &str,
    verify_checksum: bool,
) -> Result<PathBuf, GeoEngineError> {
    let asset_path = asset_dir.join(asset_name);

    // Check if file exists
    if asset_path.exists() {
        // If checksum verification is enabled, verify it
        if verify_checksum {
            let file_sha256 = compute_file_sha256(&asset_path)?;
            if file_sha256 == expected_sha256 {
                return Ok(asset_path);
            } else {
                // Checksum mismatch - will redownload below
                eprintln!(
                    "Checksum mismatch for {}: expected {}, got {}. Redownloading...",
                    asset_name, expected_sha256, file_sha256
                );
            }
        } else {
            // File exists and no checksum verification needed
            return Ok(asset_path);
        }
    }

    // File doesn't exist or checksum failed - download it
    let bytes = download_bytes(url, asset_name)?;

    // Verify checksum if requested
    if verify_checksum {
        let file_sha256 = compute_data_sha256(&bytes);
        if file_sha256 != expected_sha256 {
            return Err(GeoEngineError::ReleaseChecksumMismatch {
                path: asset_path.clone(),
                expected: expected_sha256.to_string(),
                actual: file_sha256,
            });
        }
    }

    // Keep old file intact until new content is fully downloaded and verified.
    write_asset_atomically(&asset_path, &bytes)?;

    Ok(asset_path)
}

fn write_asset_atomically(asset_path: &Path, bytes: &[u8]) -> Result<(), GeoEngineError> {
    let file_name = asset_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let temp_path =
        asset_path.with_file_name(format!("{}.download.{}.tmp", file_name, std::process::id()));

    fs::write(&temp_path, bytes).map_err(|source| GeoEngineError::DatabaseOpen {
        path: temp_path.clone(),
        source,
    })?;

    match fs::rename(&temp_path, asset_path) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            if asset_path.exists() {
                let _ = fs::remove_file(asset_path);
                fs::rename(&temp_path, asset_path).map_err(|source| GeoEngineError::DatabaseOpen {
                    path: asset_path.to_path_buf(),
                    source,
                })
            } else {
                let _ = fs::remove_file(&temp_path);
                Err(GeoEngineError::DatabaseOpen {
                    path: asset_path.to_path_buf(),
                    source: rename_err,
                })
            }
        }
    }
}

fn compute_file_sha256(path: &Path) -> Result<String, GeoEngineError> {
    let mut file = fs::File::open(path).map_err(|source| GeoEngineError::DatabaseOpen {
        path: path.to_path_buf(),
        source,
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|source| GeoEngineError::DatabaseOpen {
                path: path.to_path_buf(),
                source,
            })?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn compute_data_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn download_bytes(url: &str, asset_name: &str) -> Result<Vec<u8>, GeoEngineError> {
    let client = http_client();
    client
        .get(url)
        .send()
        .map_err(|source| GeoEngineError::ReleaseDownloadFailed {
            asset: asset_name.to_string(),
            source,
        })?
        .error_for_status()
        .map_err(|source| GeoEngineError::ReleaseDownloadFailed {
            asset: asset_name.to_string(),
            source,
        })?
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|source| GeoEngineError::ReleaseDownloadFailed {
            asset: asset_name.to_string(),
            source,
        })
}

fn cache_dir() -> Result<PathBuf, GeoEngineError> {
    if let Some(custom) = env::var_os("GEO_ENGINE_CACHE_DIR") {
        return Ok(PathBuf::from(custom));
    }

    if cfg!(target_os = "macos")
        && let Some(home) = env::var_os("HOME")
    {
        return Ok(PathBuf::from(home).join("Library/Caches/geo_engine"));
    }

    if let Some(xdg_cache) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(xdg_cache).join("geo_engine"));
    }

    if let Some(home) = env::var_os("HOME") {
        return Ok(PathBuf::from(home).join(".cache/geo_engine"));
    }

    Ok(env::temp_dir().join("geo_engine"))
}

fn http_client() -> Client {
    Client::builder()
        .user_agent("geo_engine-bootstrap/1.0")
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build HTTP client")
}
