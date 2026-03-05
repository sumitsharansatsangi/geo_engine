use std::error::Error;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum GeoEngineError {
    DatabaseOpen { path: PathBuf, source: std::io::Error },
    DatabaseMap { path: PathBuf, source: std::io::Error },
    CountryNotFound { lat: f32, lon: f32 },
    StateDatabaseUnavailable { path: PathBuf, source: std::io::Error },
    StateNotFound { lat: f32, lon: f32 },
}

impl fmt::Display for GeoEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DatabaseOpen { path, source } => {
                write!(f, "failed to open database '{}': {}", path.display(), source)
            }
            Self::DatabaseMap { path, source } => {
                write!(f, "failed to memory-map database '{}': {}", path.display(), source)
            }
            Self::CountryNotFound { lat, lon } => {
                write!(f, "no country found at coordinates lat={}, lon={}", lat, lon)
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
        }
    }
}

impl Error for GeoEngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DatabaseOpen { source, .. } => Some(source),
            Self::DatabaseMap { source, .. } => Some(source),
            Self::StateDatabaseUnavailable { source, .. } => Some(source),
            Self::CountryNotFound { .. } | Self::StateNotFound { .. } => None,
        }
    }
}
