use crate::engine::api::InitializedGeoEngine;
use crate::engine::error::GeoEngineError;
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const GITHUB_REPO: &str = "sumitsharansatsangi/geo_engine";
const GITHUB_API_URL: &str =
    "https://api.github.com/repos/sumitsharansatsangi/geo_engine/releases/latest";

const DB_EXT: &str = "db";
const FST_EXT: &str = "fst";
const RKYV_EXT: &str = "rkyv";
const POINTS_EXT: &str = "points";

// Fallback to v0.0.1 if API fails
const FALLBACK_VERSION: &str = "0.0.1";
const FALLBACK_GEO_DB_SHA256: &str =
    "44c2b0887d044135336538f0f67df3d49f2e8b64d04d4b2b3c03fb6d946f7fa0";
const FALLBACK_SUBDISTRICT_DB_SHA256: &str =
    "72ce3c7c8e3cfdea2d354172c4d5536044b05e8d2b91a5a2dda72326fb0291aa";
const FALLBACK_CITY_FST_SHA256: &str =
    "8bb3a2f202db0864537e8ebd3bdc31c229218ca06a8ca787df5b7d7112a51995";
const FALLBACK_CITY_RKYV_SHA256: &str =
    "7da471653c444d3b1b16070a33819653f04f9f100a1065b951e89b86d6e1a6fb";
const FALLBACK_CITY_POINTS_SHA256: &str =
    "ac5836cf4a7a0bd93a96638830bcba546c61eec59b13ebf8317bfafdf3d0b46e";

/// Asset information from GitHub release
#[derive(Debug, Clone)]
struct AssetInfo {
    pub name: String,
    pub url: String,
    pub checksum: String,
}

/// Release information from GitHub API
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    pub tag_name: String,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}

/// Maps assets from a release, extracting files and their checksums
struct ReleaseAssets {
    pub geo_db: AssetInfo,
    pub subdistrict_db: AssetInfo,
    pub city_fst: AssetInfo,
    pub city_rkyv: AssetInfo,
    pub city_points: AssetInfo,
}

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

    // Fetch latest release information (or use fallback if API fails)
    let assets = fetch_release_assets().unwrap_or_else(|_| {
        eprintln!(
            "geo_engine: Failed to fetch latest release, using fallback v{}",
            FALLBACK_VERSION
        );
        get_fallback_release_assets()
    });

    let geo_db_path = ensure_asset(
        &config.asset_dir,
        &assets.geo_db.name,
        &assets.geo_db.url,
        &assets.geo_db.checksum,
        config.verify_checksum,
    )?;
    let subdistrict_db_path = ensure_asset(
        &config.asset_dir,
        &assets.subdistrict_db.name,
        &assets.subdistrict_db.url,
        &assets.subdistrict_db.checksum,
        config.verify_checksum,
    )?;
    let city_fst_path = ensure_asset(
        &config.asset_dir,
        &assets.city_fst.name,
        &assets.city_fst.url,
        &assets.city_fst.checksum,
        config.verify_checksum,
    )?;
    let city_rkyv_path = ensure_asset(
        &config.asset_dir,
        &assets.city_rkyv.name,
        &assets.city_rkyv.url,
        &assets.city_rkyv.checksum,
        config.verify_checksum,
    )?;
    let city_points_path = ensure_asset(
        &config.asset_dir,
        &assets.city_points.name,
        &assets.city_points.url,
        &assets.city_points.checksum,
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
            InitializedGeoEngine::open(
                &paths.geo_db_path,
                &paths.subdistrict_db_path,
                &paths.city_fst_path,
                &paths.city_rkyv_path,
            )
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

    let paths = init_all_assets_with_config(config)?;
    InitializedGeoEngine::open(
        &paths.geo_db_path,
        &paths.subdistrict_db_path,
        &paths.city_fst_path,
        &paths.city_rkyv_path,
    )
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

    // Fetch latest release information (or use fallback if API fails)
    let assets = fetch_release_assets().unwrap_or_else(|_| {
        eprintln!(
            "geo_engine: Failed to fetch latest release, using fallback v{}",
            FALLBACK_VERSION
        );
        get_fallback_release_assets()
    });

    let fst_path = ensure_asset(
        &config.asset_dir,
        &assets.city_fst.name,
        &assets.city_fst.url,
        &assets.city_fst.checksum,
        config.verify_checksum,
    )?;
    let rkyv_path = ensure_asset(
        &config.asset_dir,
        &assets.city_rkyv.name,
        &assets.city_rkyv.url,
        &assets.city_rkyv.checksum,
        config.verify_checksum,
    )?;
    let points_path = ensure_asset(
        &config.asset_dir,
        &assets.city_points.name,
        &assets.city_points.url,
        &assets.city_points.checksum,
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

/// Fetch the latest release from GitHub and extract asset information
fn fetch_release_assets() -> Result<ReleaseAssets, GeoEngineError> {
    let client = http_client();

    let release: GitHubRelease = {
        let response = client
            .get(GITHUB_API_URL)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .map_err(|source| GeoEngineError::ReleaseMetadataUnavailable {
                repo: GITHUB_REPO.to_string(),
                source,
            })?;

        let text =
            response
                .text()
                .map_err(|source| GeoEngineError::ReleaseMetadataUnavailable {
                    repo: GITHUB_REPO.to_string(),
                    source,
                })?;

        serde_json::from_str(&text).map_err(|source| GeoEngineError::ReleaseMetadataParse {
            repo: GITHUB_REPO.to_string(),
            source,
        })?
    };

    extract_assets_from_release(&release)
}

/// Extract asset information from a GitHub release
fn extract_assets_from_release(release: &GitHubRelease) -> Result<ReleaseAssets, GeoEngineError> {
    let version = release.tag_name.trim_start_matches('v').to_string();

    // Build a map of assets by name for easy lookup
    let asset_map: HashMap<&str, &GitHubAsset> = release
        .assets
        .iter()
        .filter_map(|asset| {
            // Extract base name (e.g., "geo-0.0.1" from "geo-0.0.1.db")
            if let Some(pos) = asset.name.rfind('.') {
                let base = &asset.name[..pos];
                Some((base, asset))
            } else {
                None
            }
        })
        .collect();

    // Find assets for each file type
    let geo_db = find_asset_by_pattern(&asset_map, "geo", DB_EXT, &version)?;
    let subdistrict_db = find_asset_by_pattern(&asset_map, "subdistrict", DB_EXT, &version)?;
    let city_fst = find_asset_by_pattern(&asset_map, "cities", FST_EXT, &version)?;
    let city_rkyv = find_asset_by_pattern(&asset_map, "cities", RKYV_EXT, &version)?;
    let city_points = find_asset_by_pattern(&asset_map, "cities", POINTS_EXT, &version)?;

    Ok(ReleaseAssets {
        geo_db,
        subdistrict_db,
        city_fst,
        city_rkyv,
        city_points,
    })
}

/// Find an asset in the release by pattern matching
fn find_asset_by_pattern(
    assets: &HashMap<&str, &GitHubAsset>,
    base_name: &str,
    ext: &str,
    version: &str,
) -> Result<AssetInfo, GeoEngineError> {
    // Try to match asset name pattern: "base_name-version.ext"
    let expected_name = format!("{}-{}.{}", base_name, version, ext);

    if let Some(asset) = assets
        .values()
        .find(|a| a.name.ends_with(&format!(".{}", ext)) && a.name.contains(base_name))
    {
        // Try to fetch checksum from .sha256 file
        let checksum = fetch_checksum_for_asset(&asset.name).unwrap_or_else(|_| {
            eprintln!(
                "geo_engine: Could not fetch checksum for {}, skipping verification",
                asset.name
            );
            String::new()
        });

        return Ok(AssetInfo {
            name: asset.name.clone(),
            url: asset.browser_download_url.clone(),
            checksum,
        });
    }

    Err(GeoEngineError::ReleaseAssetMissing {
        repo: GITHUB_REPO.to_string(),
        asset: expected_name,
    })
}

/// Try to fetch the SHA256 checksum for an asset
fn fetch_checksum_for_asset(asset_name: &str) -> Result<String, GeoEngineError> {
    // Construct checksum URL from asset URL
    // e.g., geo-0.0.1.db.sha256
    let checksum_url = format!(
        "https://raw.githubusercontent.com/{}/main/{}.sha256",
        GITHUB_REPO, asset_name
    );

    let client = http_client();
    let response = client.get(&checksum_url).send().map_err(|source| {
        GeoEngineError::ReleaseDownloadFailed {
            asset: format!("{}.sha256", asset_name),
            source,
        }
    })?;

    let checksum_text =
        response
            .text()
            .map_err(|source| GeoEngineError::ReleaseDownloadFailed {
                asset: format!("{}.sha256", asset_name),
                source,
            })?;

    // Extract just the hash (first word before space)
    Ok(checksum_text
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string())
}

/// Get fallback assets for v0.0.1 (in case API fails)
fn get_fallback_release_assets() -> ReleaseAssets {
    let version = FALLBACK_VERSION.to_string();

    ReleaseAssets {
        geo_db: AssetInfo {
            name: format!("geo-{}.db", version),
            url: format!(
                "https://github.com/{}/releases/download/v{}/geo-{}.db",
                GITHUB_REPO, version, version
            ),
            checksum: FALLBACK_GEO_DB_SHA256.to_string(),
        },
        subdistrict_db: AssetInfo {
            name: format!("subdistrict-{}.db", version),
            url: format!(
                "https://github.com/{}/releases/download/v{}/subdistrict-{}.db",
                GITHUB_REPO, version, version
            ),
            checksum: FALLBACK_SUBDISTRICT_DB_SHA256.to_string(),
        },
        city_fst: AssetInfo {
            name: format!("cities-{}.fst", version),
            url: format!(
                "https://github.com/{}/releases/download/v{}/cities-{}.fst",
                GITHUB_REPO, version, version
            ),
            checksum: FALLBACK_CITY_FST_SHA256.to_string(),
        },
        city_rkyv: AssetInfo {
            name: format!("cities-{}.rkyv", version),
            url: format!(
                "https://github.com/{}/releases/download/v{}/cities-{}.rkyv",
                GITHUB_REPO, version, version
            ),
            checksum: FALLBACK_CITY_RKYV_SHA256.to_string(),
        },
        city_points: AssetInfo {
            name: format!("cities-{}.points", version),
            url: format!(
                "https://github.com/{}/releases/download/v{}/cities-{}.points",
                GITHUB_REPO, version, version
            ),
            checksum: FALLBACK_CITY_POINTS_SHA256.to_string(),
        },
    }
}

fn http_client() -> Client {
    Client::builder()
        .user_agent("geo_engine-bootstrap/1.0")
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build HTTP client")
}
