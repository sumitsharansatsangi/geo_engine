use std::error::Error;
use std::fmt;
use std::path::PathBuf;

/// Errors that can occur during geo engine operations.
///
/// This enum covers database access errors, geographic lookup failures,
/// asset initialization errors, and path management issues.
#[derive(Debug)]
pub enum GeoEngineError {
    /// Failed to open a database file.
    DatabaseOpen {
        path: PathBuf,
        source: std::io::Error,
    },
    /// Failed to memory-map a database file.
    DatabaseMap {
        path: PathBuf,
        source: std::io::Error,
    },
    /// No country found at the given coordinates.
    CountryNotFound { lat: f32, lon: f32 },
    /// District database is unavailable for the requested operation.
    DistrictDatabaseUnavailable {
        path: PathBuf,
        source: std::io::Error,
    },
    /// No district found at the given coordinates.
    DistrictNotFound { lat: f32, lon: f32 },
    /// Cache directory could not be created or accessed.
    CacheDirectoryUnavailable {
        path: PathBuf,
        source: std::io::Error,
    },
    /// Failed to fetch GitHub release metadata.
    ReleaseMetadataUnavailable {
        repo: String,
        source: reqwest::Error,
    },
    /// Failed to parse GitHub release metadata.
    ReleaseMetadataParse {
        repo: String,
        source: serde_json::Error,
    },
    /// Failed to parse release manifest.
    ReleaseManifestParse {
        repo: String,
        source: serde_json::Error,
    },
    /// Expected asset not found in release.
    ReleaseAssetMissing { repo: String, asset: String },
    /// Failed to download release asset.
    ReleaseDownloadFailed {
        asset: String,
        source: reqwest::Error,
    },
    /// Checksum verification failed for downloaded asset.
    ReleaseChecksumMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    /// Paths have not been initialized (call init_path first).
    PathsNotInitialized,
    /// Paths are already initialized with different values.
    PathsAlreadyInitialized,
    /// Engine initialization failed with the given reason.
    EngineInitializationFailed { message: String },
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
            Self::DistrictNotFound { lat, lon } => {
                write!(
                    f,
                    "no district found at coordinates lat={}, lon={}",
                    lat, lon
                )
            }
            Self::DistrictDatabaseUnavailable { path, source } => {
                write!(
                    f,
                    "district lookup required but database '{}' is unavailable: {}",
                    path.display(),
                    source
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
                    repo, source
                )
            }
            Self::ReleaseMetadataParse { repo, source } => {
                write!(
                    f,
                    "failed to parse latest release metadata from '{}': {}",
                    repo, source
                )
            }
            Self::ReleaseManifestParse { repo, source } => {
                write!(
                    f,
                    "failed to parse release manifest for '{}': {}",
                    repo, source
                )
            }
            Self::ReleaseAssetMissing { repo, asset } => {
                write!(
                    f,
                    "release asset '{}' was not found in latest release for '{}'",
                    asset, repo
                )
            }
            Self::ReleaseDownloadFailed { asset, source } => {
                write!(
                    f,
                    "failed to download release asset '{}': {}",
                    asset, source
                )
            }
            Self::ReleaseChecksumMismatch {
                path,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "checksum mismatch for '{}': expected {}, got {}",
                    path.display(),
                    expected,
                    actual
                )
            }
            Self::PathsNotInitialized => {
                write!(
                    f,
                    "path configuration is not initialized; call init_path first"
                )
            }
            Self::PathsAlreadyInitialized => {
                write!(f, "path configuration has already been initialized")
            }
            Self::EngineInitializationFailed { message } => {
                write!(f, "engine initialization failed: {}", message)
            }
        }
    }
}

impl Error for GeoEngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DatabaseOpen { source, .. } => Some(source),
            Self::DatabaseMap { source, .. } => Some(source),
            Self::DistrictDatabaseUnavailable { source, .. } => Some(source),
            Self::CacheDirectoryUnavailable { source, .. } => Some(source),
            Self::ReleaseMetadataUnavailable { source, .. } => Some(source),
            Self::ReleaseMetadataParse { source, .. } => Some(source),
            Self::ReleaseManifestParse { source, .. } => Some(source),
            Self::ReleaseDownloadFailed { source, .. } => Some(source),
            Self::CountryNotFound { .. }
            | Self::DistrictNotFound { .. }
            | Self::ReleaseAssetMissing { .. }
            | Self::ReleaseChecksumMismatch { .. }
            | Self::PathsNotInitialized
            | Self::PathsAlreadyInitialized
            | Self::EngineInitializationFailed { .. } => None,
        }
    }
}
