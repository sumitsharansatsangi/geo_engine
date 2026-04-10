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
    EngineNotInitialized,
    EngineAlreadyInitialized {
        country_path: PathBuf,
        subdistrict_path: PathBuf,
    },
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
            Self::EngineNotInitialized => {
                write!(f, "geo engine has not been initialized")
            }
            Self::EngineAlreadyInitialized {
                country_path,
                subdistrict_path,
            } => {
                write!(
                    f,
                    "geo engine is already initialized with country db '{}' and subdistrict db '{}'",
                    country_path.display(),
                    subdistrict_path.display()
                )
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
            Self::CountryNotFound { .. }
            | Self::StateNotFound { .. }
            | Self::DistrictNotFound { .. }
            | Self::EngineNotInitialized
            | Self::EngineAlreadyInitialized { .. } => None,
        }
    }
}
