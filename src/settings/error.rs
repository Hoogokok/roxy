use std::fmt;
use crate::settings::typestate::ValidationErrorCollector;

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
    FileNotFound {
        path: String,
        context: String,
    },
    FileAccessError {
        path: String,
        context: String,
        error: String,
    },
    MissingField {
        field: String,
        context: String,
    },
    InvalidValue {
        field: String,
        context: String,
        message: String,
    },
    ParseError {
        source: toml::de::Error,
    },
    JsonParseError {
        source: serde_json::Error,
    },
    InvalidConfig(String),
    DuplicateMiddleware(String),
    WatchError(String),
    SchemaCompileError {
        reason: String,
    },
    ValidationErrors {
        errors: Vec<String>,
        file: String,
    },
    ValidationError {
        field: String,
        message: String,
    },
    ReferenceError {
        entity: String,
        reference: String,
        message: String,
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
            Self::FileNotFound { path, context } => 
                write!(f, "파일 {} 찾을 수 없음: {}", path, context),
            Self::FileAccessError { path, context, error } => 
                write!(f, "파일 {} 접근 권한 오류: {}: {}", path, context, error),
            Self::MissingField { field, context } => 
                write!(f, "필수 필드 {} 누락: {}", field, context),
            Self::InvalidValue { field, context, message } => 
                write!(f, "유효하지 않은 값 {} 오류: {}", field, message),
            Self::ParseError { source } => 
                write!(f, "TOML 설정 파싱 오류: {}", source),
            Self::JsonParseError { source } => 
                write!(f, "JSON 설정 파싱 오류: {}", source),
            Self::InvalidConfig(msg) => write!(f, "Invalid configuration: {}", msg),
            Self::DuplicateMiddleware(name) => write!(f, "Duplicate middleware: {}", name),
            Self::WatchError(msg) => write!(f, "Watch error: {}", msg),
            Self::SchemaCompileError { reason } => 
                write!(f, "스키마 컴파일 오류: {}", reason),
            Self::ValidationErrors { errors, file } => {
                writeln!(f, "설정 파일 {} 검증 오류:", file)?;
                for (i, error) in errors.iter().enumerate() {
                    writeln!(f, "  {}. {}", i+1, error)?;
                }
                Ok(())
            },
            Self::ValidationError { field, message } =>
                write!(f, "필드 '{}' 검증 오류: {}", field, message),
            Self::ReferenceError { entity, reference, message } => 
                write!(f, "참조 오류 - 엔티티: {}, 참조: {}, 이유: {}", entity, reference, message),
        }
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ParseError { source } => Some(source),
            Self::JsonParseError { source } => Some(source),
            Self::FileError { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SettingsError {
    fn from(err: std::io::Error) -> Self {
        SettingsError::FileError {
            path: String::new(),
            error: err,
        }
    }
}

impl From<toml::de::Error> for SettingsError {
    fn from(err: toml::de::Error) -> Self {
        SettingsError::ParseError {
            source: err,
        }
    }
}

/// 여러 설정 오류를 수집하고 관리하는 검증기
#[derive(Debug, Default)]
pub struct SettingsValidator {
    /// 수집된 오류 목록
    errors: Vec<SettingsError>,
    
    /// 현재 오류 수집 중인지 여부
    collecting: bool,
}

impl SettingsValidator {
    /// 새로운 설정 검증기 생성
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            collecting: false,
        }
    }
    
    /// 특정 필드에 대한 검증 오류 추가
    pub fn add_field_error(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.add_error(SettingsError::ValidationError {
            field: field.into(),
            message: message.into(),
        });
    }
    
    /// 참조 오류 추가
    pub fn add_reference_error(
        &mut self, 
        entity: impl Into<String>, 
        reference: impl Into<String>, 
        message: impl Into<String>
    ) {
        self.add_error(SettingsError::ReferenceError {
            entity: entity.into(),
            reference: reference.into(),
            message: message.into(),
        });
    }
    
    /// 전체 오류 목록 반환
    pub fn into_errors(self) -> Vec<SettingsError> {
        self.errors
    }
    
    /// ValidationErrors로 변환
    pub fn into_error(self) -> SettingsError {
        SettingsError::ValidationErrors {
            errors: self.errors.iter().map(|e| format!("{}", e)).collect(),
            file: String::new(),
        }
    }
    
    /// 오류 목록을 파일 검증 오류로 변환
    pub fn into_validation_errors(self, file: impl Into<String>) -> SettingsError {
        SettingsError::ValidationErrors {
            errors: self.errors.iter().map(|e| format!("{}", e)).collect(),
            file: file.into(),
        }
    }
}

impl ValidationErrorCollector for SettingsValidator {
    type Error = SettingsError;
    
    fn start_collecting(&mut self) {
        self.collecting = true;
    }
    
    fn add_error(&mut self, error: Self::Error) {
        self.errors.push(error);
    }
    
    fn get_errors(&self) -> Vec<&Self::Error> {
        self.errors.iter().collect()
    }
    
    fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    
    fn handle_errors<T>(&self) -> Result<T, Vec<Self::Error>> {
        if self.has_errors() {
            // 오류들을 복제해서 반환
            let errors = self.errors.iter()
                .map(|e| e.clone_error())
                .collect();
            
            Err(errors)
        } else {
            panic!("오류가 없는데 handle_errors를 호출했습니다")
        }
    }
}

// SettingsError 클론 지원 확장
impl SettingsError {
    /// 오류를 복제하는 메서드
    pub fn clone_error(&self) -> Self {
        match self {
            Self::EnvVarMissing { var_name } => Self::EnvVarMissing {
                var_name: var_name.clone(),
            },
            Self::EnvVarInvalid { var_name, value, reason } => Self::EnvVarInvalid {
                var_name: var_name.clone(),
                value: value.clone(),
                reason: reason.clone(),
            },
            Self::FileError { path, .. } => Self::FileError {
                path: path.clone(),
                // 오류 객체를 새로 생성하고 메시지만 유지
                error: std::io::Error::new(
                    std::io::ErrorKind::Other, 
                    format!("복제된 IO 오류: {}", path)
                ),
            },
            Self::FileNotFound { path, context } => Self::FileNotFound {
                path: path.clone(),
                context: context.clone(),
            },
            Self::FileAccessError { path, context, error } => Self::FileAccessError {
                path: path.clone(),
                context: context.clone(),
                error: error.clone(),
            },
            Self::MissingField { field, context } => Self::MissingField {
                field: field.clone(),
                context: context.clone(),
            },
            Self::InvalidValue { field, context, message } => Self::InvalidValue {
                field: field.clone(),
                context: context.clone(),
                message: message.clone(),
            },
            Self::ParseError { source } => Self::InvalidConfig(
                format!("TOML 파싱 오류: {}", source)
            ),
            Self::JsonParseError { source } => Self::InvalidConfig(
                format!("JSON 파싱 오류: {}", source)
            ),
            Self::InvalidConfig(msg) => Self::InvalidConfig(msg.clone()),
            Self::DuplicateMiddleware(name) => Self::DuplicateMiddleware(name.clone()),
            Self::WatchError(msg) => Self::WatchError(msg.clone()),
            Self::SchemaCompileError { reason } => Self::SchemaCompileError {
                reason: reason.clone(),
            },
            Self::ValidationErrors { errors, file } => Self::ValidationErrors {
                errors: errors.clone(),
                file: file.clone(),
            },
            Self::ValidationError { field, message } => Self::ValidationError {
                field: field.clone(),
                message: message.clone(),
            },
            Self::ReferenceError { entity, reference, message } => Self::ReferenceError {
                entity: entity.clone(),
                reference: reference.clone(),
                message: message.clone(),
            },
        }
    }
}

impl Clone for SettingsError {
    fn clone(&self) -> Self {
        self.clone_error()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_settings_validator_basic() {
        let mut validator = SettingsValidator::new();
        assert!(!validator.has_errors());
        
        validator.add_field_error("port", "포트 값이 범위를 벗어났습니다");
        assert!(validator.has_errors());
        assert_eq!(validator.get_errors().len(), 1);
        
        if let SettingsError::ValidationError { field, message } = &validator.get_errors()[0] {
            assert_eq!(field, "port");
            assert_eq!(message, "포트 값이 범위를 벗어났습니다");
        } else {
            panic!("잘못된 오류 타입");
        }
    }
    
    #[test]
    fn test_settings_validator_multiple_errors() {
        let mut validator = SettingsValidator::new();
        
        validator.add_field_error("port", "포트 값이 범위를 벗어났습니다");
        validator.add_field_error("host", "호스트가 비어 있습니다");
        validator.add_reference_error("router", "middleware", "존재하지 않는 미들웨어 참조");
        
        assert_eq!(validator.get_errors().len(), 3);
        
        // ValidationErrors로 변환
        let file_error = validator.into_validation_errors("config.toml");
        if let SettingsError::ValidationErrors { errors, file } = file_error {
            assert_eq!(errors.len(), 3);
            assert_eq!(file, "config.toml");
        } else {
            panic!("잘못된 오류 타입");
        }
    }
    
    #[test]
    fn test_validation_error_collector() {
        let mut validator = SettingsValidator::new();
        validator.start_collecting();
        
        validator.add_field_error("port", "포트 값이 범위를 벗어났습니다");
        validator.add_field_error("host", "호스트가 비어 있습니다");
        
        assert!(validator.has_errors());
        
        // handle_errors 사용
        let result: Result<(), Vec<SettingsError>> = validator.handle_errors();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().len(), 2);
    }
} 