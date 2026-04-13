use std::error::Error;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum GeoEngineError {
    DatabaseOpen {
        path: PathBuf,
        source: std::io::Error,
    },
    DatabaseMap {
        path: PathBuf,
        source: std::io::Error,
    },
    CountryNotFound {
        lat: f32,
        lon: f32,
    },
    StateDatabaseUnavailable {
        path: PathBuf,
        source: std::io::Error,
    },
    StateNotFound {
        lat: f32,
        lon: f32,
    },
    DistrictDatabaseUnavailable {
        path: PathBuf,
        source: std::io::Error,
    },
    DistrictNotFound {
        lat: f32,
        lon: f32,
    },
    CacheDirectoryUnavailable {
        path: PathBuf,
        source: std::io::Error,
    },
    ReleaseMetadataUnavailable {
        repo: String,
        source: reqwest::Error,
    },
    ReleaseMetadataParse {
        repo: String,
        source: serde_json::Error,
    },
    ReleaseAssetMissing {
        repo: String,
        asset: String,
    },
    ReleaseDownloadFailed {
        asset: String,
        source: reqwest::Error,
    },
    ReleaseChecksumMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    PathsNotInitialized,
    PathsAlreadyInitialized,
    SubdistrictPathNotInitialized,
}

impl fmt::Display for GeoEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DatabaseOpen { path, source } => {
                write!(
                    f,
                    "failed to open database '{}': {}",
                    path.display(),
                    source
                )
            }
            Self::DatabaseMap { path, source } => {
                write!(
                    f,
                    "failed to memory-map database '{}': {}",
                    path.display(),
                    source
                )
            }
            Self::CountryNotFound { lat, lon } => {
                write!(
                    f,
                    "no country found at coordinates lat={}, lon={}",
                    lat, lon
                )
            }
            Self::StateDatabaseUnavailable { path, source } => {
                write!(
                    f,
                    "state lookup required but database '{}' is unavailable: {}",
                    path.display(),
                    source
                )
            }
            Self::StateNotFound { lat, lon } => {
                write!(f, "no state found at coordinates lat={}, lon={}", lat, lon)
            }
            Self::DistrictDatabaseUnavailable { path, source } => {
                write!(
                    f,
                    "district lookup required but database '{}' is unavailable: {}",
                    path.display(),
                    source
                )
            }
            Self::DistrictNotFound { lat, lon } => {
                write!(
                    f,
                    "no district found at coordinates lat={}, lon={}",
                    lat, lon
                )
            }
            Self::CacheDirectoryUnavailable { path, source } => {
                write!(
                    f,
                    "failed to prepare cache directory '{}': {}",
                    path.display(),
                    source
                )
            }
            Self::ReleaseMetadataUnavailable { repo, source } => {
                write!(
                    f,
                    "failed to fetch latest release metadata from '{}': {}",
                    repo,
                    source
                )
            }
            Self::ReleaseMetadataParse { repo, source } => {
                write!(
                    f,
                    "failed to parse latest release metadata from '{}': {}",
                    repo,
                    source
                )
            }
            Self::ReleaseAssetMissing { repo, asset } => {
                write!(
                    f,
                    "release asset '{}' was not found in latest release for '{}'",
                    asset,
                    repo
                )
            }
            Self::ReleaseDownloadFailed { asset, source } => {
                write!(
                    f,
                    "failed to download release asset '{}': {}",
                    asset,
                    source
                )
            }
            Self::ReleaseChecksumMismatch { path, expected, actual } => {
                write!(
                    f,
                    "checksum mismatch for '{}': expected {}, got {}",
                    path.display(),
                    expected,
                    actual
                )
            }
            Self::PathsNotInitialized => {
                write!(f, "path configuration is not initialized; call init_path first")
            }
            Self::PathsAlreadyInitialized => {
                write!(f, "path configuration has already been initialized")
            }
            Self::SubdistrictPathNotInitialized => {
                write!(f, "subdistrict path is required but was not initialized")
            }
        }
    }
}

impl Error for GeoEngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DatabaseOpen { source, .. } => Some(source),
            Self::DatabaseMap { source, .. } => Some(source),
            Self::StateDatabaseUnavailable { source, .. } => Some(source),
            Self::DistrictDatabaseUnavailable { source, .. } => Some(source),
            Self::CacheDirectoryUnavailable { source, .. } => Some(source),
            Self::ReleaseMetadataUnavailable { source, .. } => Some(source),
            Self::ReleaseMetadataParse { source, .. } => Some(source),
            Self::ReleaseDownloadFailed { source, .. } => Some(source),
            Self::CountryNotFound { .. }
            | Self::StateNotFound { .. }
            | Self::DistrictNotFound { .. }
            | Self::ReleaseAssetMissing { .. }
            | Self::ReleaseChecksumMismatch { .. }
            | Self::PathsNotInitialized
            | Self::PathsAlreadyInitialized
            | Self::SubdistrictPathNotInitialized => None,
        }
    }
}
