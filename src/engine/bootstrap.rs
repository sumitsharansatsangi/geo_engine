use crate::engine::api::InitializedGeoEngine;
use crate::engine::error::GeoEngineError;
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const GITHUB_REPO: &str = "sumitsharansatsangi/geo_engine";
const GITHUB_API_URL: &str =
    "https://api.github.com/repos/sumitsharansatsangi/geo_engine/releases/latest";
const RELEASE_MANIFEST_ASSET_NAME: &str = "assets-manifest.json";

#[derive(Debug, Clone)]
struct AssetInfo {
    pub name: String,
    pub url: String,
    pub checksum: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}

struct ReleaseAssets {
    pub geo_db: Option<AssetInfo>,
    pub subdistrict_db: Option<AssetInfo>,
    pub city_fst: AssetInfo,
    pub city_rkyv: AssetInfo,
    pub city_points: AssetInfo,
}

enum RequiredAssetGroup {
    All,
    City,
}

#[derive(Debug, Deserialize)]
struct ReleaseManifest {
    geo: Option<ManifestGeoGroup>,
    subdistrict: Option<ManifestSubdistrictGroup>,
    city: Option<ManifestCityGroup>,
}

#[derive(Debug, Deserialize)]
struct ManifestGeoGroup {
    db: ManifestAsset,
}

#[derive(Debug, Deserialize)]
struct ManifestSubdistrictGroup {
    db: ManifestAsset,
}

#[derive(Debug, Deserialize)]
struct ManifestCityGroup {
    fst: ManifestAsset,
    rkyv: ManifestAsset,
    points: ManifestAsset,
}

#[derive(Debug, Deserialize)]
struct ManifestAsset {
    name: String,
    url: String,
    sha256: String,
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

    let assets = fetch_release_assets(RequiredAssetGroup::All)?;

    let geo_asset = assets
        .geo_db
        .ok_or_else(|| GeoEngineError::ReleaseAssetMissing {
            repo: GITHUB_REPO.to_string(),
            asset: "geo.db".to_string(),
        })?;
    let subdistrict_asset =
        assets
            .subdistrict_db
            .ok_or_else(|| GeoEngineError::ReleaseAssetMissing {
                repo: GITHUB_REPO.to_string(),
                asset: "subdistrict.db".to_string(),
            })?;

    let geo_db_path = ensure_asset(
        &config.asset_dir,
        &geo_asset.name,
        &geo_asset.url,
        &geo_asset.checksum,
        config.verify_checksum,
    )?;
    let subdistrict_db_path = ensure_asset(
        &config.asset_dir,
        &subdistrict_asset.name,
        &subdistrict_asset.url,
        &subdistrict_asset.checksum,
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

    let assets = fetch_release_assets(RequiredAssetGroup::City)?;

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

    if asset_path.exists() {
        if verify_checksum {
            let file_sha256 = compute_file_sha256(&asset_path)?;
            if file_sha256 == expected_sha256 {
                return Ok(asset_path);
            }

            eprintln!(
                "Checksum mismatch for {}: expected {}, got {}. Redownloading...",
                asset_name, expected_sha256, file_sha256
            );
        } else {
            return Ok(asset_path);
        }
    }

    let bytes = download_bytes(url, asset_name)?;

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

    Ok(digest_to_hex(hasher.finalize().as_slice()))
}

fn compute_data_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    digest_to_hex(hasher.finalize().as_slice())
}

fn digest_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
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

fn fetch_release_assets(required: RequiredAssetGroup) -> Result<ReleaseAssets, GeoEngineError> {
    let release = fetch_latest_release()?;
    let manifest_url = release_manifest_url(&release)?;
    let manifest = download_release_manifest(&manifest_url)?;

    let city = manifest
        .city
        .ok_or_else(|| GeoEngineError::ReleaseAssetMissing {
            repo: GITHUB_REPO.to_string(),
            asset: "manifest.city".to_string(),
        })?;

    let city_fst = manifest_asset_to_info(city.fst, "manifest.city.fst")?;
    let city_rkyv = manifest_asset_to_info(city.rkyv, "manifest.city.rkyv")?;
    let city_points = manifest_asset_to_info(city.points, "manifest.city.points")?;

    match required {
        RequiredAssetGroup::City => Ok(ReleaseAssets {
            geo_db: None,
            subdistrict_db: None,
            city_fst,
            city_rkyv,
            city_points,
        }),
        RequiredAssetGroup::All => {
            let geo = manifest
                .geo
                .ok_or_else(|| GeoEngineError::ReleaseAssetMissing {
                    repo: GITHUB_REPO.to_string(),
                    asset: "manifest.geo".to_string(),
                })?;
            let subdistrict =
                manifest
                    .subdistrict
                    .ok_or_else(|| GeoEngineError::ReleaseAssetMissing {
                        repo: GITHUB_REPO.to_string(),
                        asset: "manifest.subdistrict".to_string(),
                    })?;

            let geo_db = manifest_asset_to_info(geo.db, "manifest.geo.db")?;
            let subdistrict_db = manifest_asset_to_info(subdistrict.db, "manifest.subdistrict.db")?;

            Ok(ReleaseAssets {
                geo_db: Some(geo_db),
                subdistrict_db: Some(subdistrict_db),
                city_fst,
                city_rkyv,
                city_points,
            })
        }
    }
}

fn fetch_latest_release() -> Result<GitHubRelease, GeoEngineError> {
    let client = http_client();

    let response = client
        .get(GITHUB_API_URL)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .map_err(|source| GeoEngineError::ReleaseMetadataUnavailable {
            repo: GITHUB_REPO.to_string(),
            source,
        })?;

    let text = response
        .text()
        .map_err(|source| GeoEngineError::ReleaseMetadataUnavailable {
            repo: GITHUB_REPO.to_string(),
            source,
        })?;

    serde_json::from_str(&text).map_err(|source| GeoEngineError::ReleaseMetadataParse {
        repo: GITHUB_REPO.to_string(),
        source,
    })
}

fn release_manifest_url(release: &GitHubRelease) -> Result<String, GeoEngineError> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == RELEASE_MANIFEST_ASSET_NAME)
        .map(|asset| asset.browser_download_url.clone())
        .ok_or_else(|| GeoEngineError::ReleaseAssetMissing {
            repo: GITHUB_REPO.to_string(),
            asset: RELEASE_MANIFEST_ASSET_NAME.to_string(),
        })
}

fn download_release_manifest(manifest_url: &str) -> Result<ReleaseManifest, GeoEngineError> {
    let manifest_bytes = download_bytes(manifest_url, RELEASE_MANIFEST_ASSET_NAME)?;

    serde_json::from_slice(&manifest_bytes).map_err(|source| GeoEngineError::ReleaseManifestParse {
        repo: GITHUB_REPO.to_string(),
        source,
    })
}

fn manifest_asset_to_info(
    asset: ManifestAsset,
    field_path: &str,
) -> Result<AssetInfo, GeoEngineError> {
    let name = asset.name.trim().to_string();
    let url = asset.url.trim().to_string();
    let checksum = asset.sha256.trim().to_ascii_lowercase();

    if name.is_empty() || url.is_empty() || checksum.len() != 64 {
        return Err(GeoEngineError::ReleaseAssetMissing {
            repo: GITHUB_REPO.to_string(),
            asset: field_path.to_string(),
        });
    }

    if !checksum.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(GeoEngineError::ReleaseAssetMissing {
            repo: GITHUB_REPO.to_string(),
            asset: field_path.to_string(),
        });
    }

    Ok(AssetInfo {
        name,
        url,
        checksum,
    })
}

fn http_client() -> Client {
    Client::builder()
        .user_agent("geo_engine-bootstrap/1.0")
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build HTTP client")
}
