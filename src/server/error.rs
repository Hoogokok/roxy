use crate::{docker::DockerError, settings::SettingsError};
use std::fmt;

#[derive(Debug)]
pub enum Error {
    ConfigError(String),
    IoError(std::io::Error),
    DockerError(DockerError),
    Other(Box<dyn std::error::Error>),
    Configuration(String),
    ConfigWatchError(String),
    ConfigWatcher(String),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IoError(err)
    }
}

impl From<DockerError> for Error {
    fn from(err: DockerError) -> Self {
        Error::DockerError(err)
    }
}

impl From<Box<dyn std::error::Error>> for Error {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        Error::Other(err)
    }
}

impl From<SettingsError> for Error {
    fn from(err: SettingsError) -> Self {
        Error::Configuration(err.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ConfigError(msg) => write!(f, "Config Error: {}", msg),
            Error::IoError(e) => write!(f, "IO Error: {}", e),
            Error::DockerError(e) => write!(f, "Docker Error: {}", e),
            Error::Other(e) => write!(f, "Error: {}", e),
            Error::Configuration(msg) => write!(f, "Configuration Error: {}", msg),
            Error::ConfigWatchError(msg) => write!(f, "Config Watch Error: {}", msg),
            Error::ConfigWatcher(msg) => write!(f, "Config Watcher Error: {}", msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::IoError(e) => Some(e),
            Error::DockerError(e) => Some(e),
            Error::Other(e) => Some(e.as_ref()),
            Error::ConfigError(_) | Error::Configuration(_) | Error::ConfigWatchError(_) => None,
            Error::ConfigWatcher(_) => None,
        }
    }
}