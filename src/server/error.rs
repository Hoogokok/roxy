use crate::docker::DockerError;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    ConfigError(String),
    IoError(std::io::Error),
    DockerError(DockerError),
    Other(Box<dyn std::error::Error>),
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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ConfigError(msg) => write!(f, "Config Error: {}", msg),
            Error::IoError(e) => write!(f, "IO Error: {}", e),
            Error::DockerError(e) => write!(f, "Docker Error: {}", e),
            Error::Other(e) => write!(f, "Error: {}", e),
        }
    }
}