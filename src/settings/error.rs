use std::fmt;

#[derive(Debug)]
pub enum SettingsError {
    EnvVarMissing {
        var_name: String,
    },
    EnvVarInvalid {
        var_name: String,
        value: String,
        reason: String,
    },
    FileError {
        path: String,
        error: std::io::Error,
    },
    ParseError {
        source: toml::de::Error,
    },
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvVarMissing { var_name } => 
                write!(f, "환경 변수 누락: {}", var_name),
            Self::EnvVarInvalid { var_name, value, reason } => 
                write!(f, "환경 변수 {} 값 {} 오류: {}", var_name, value, reason),
            Self::FileError { path, error } => 
                write!(f, "설정 파일 {} 오류: {}", path, error),
            Self::ParseError { source } => 
                write!(f, "설정 파싱 오류: {}", source),
        }
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ParseError { source } => Some(source),
            Self::FileError { error, .. } => Some(error),
            _ => None,
        }
    }
} 