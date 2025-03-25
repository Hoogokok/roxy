use serde::{Deserialize, Deserializer, de::Error};
use std::{ marker::PhantomData, path::Path, str::FromStr};
use tracing::Level;
use super::{SettingsError, typestate::{Raw, Validated, TypeState, Validatable}};
use crate::settings::error::SettingsValidator;
use crate::settings::typestate::ValidationErrorCollector;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Text,
    Json,
}

impl Default for LogFormat {
    fn default() -> Self {
        LogFormat::Text
    }
}

impl std::str::FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(LogFormat::Text),
            "json" => Ok(LogFormat::Json),
            _ => Err(format!("지원하지 않는 로그 형식: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogOutput {
    Stdout,
    File(String),
}

impl Default for LogOutput {
    fn default() -> Self {
        LogOutput::Stdout
    }
}

impl std::str::FromStr for LogOutput {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("stdout") {
            Ok(LogOutput::Stdout)
        } else if s.starts_with("file:") {
            let path = s.trim_start_matches("file:").trim();
            Ok(LogOutput::File(path.to_string()))
        } else {
            Err(format!("지원하지 않는 로그 출력 형식: {}", s))
        }
    }
}

/// 로깅 설정
/// 
/// 타입 매개변수 S는 설정의 상태를 나타냅니다:
/// - `Raw`: 검증되지 않은 원시 설정
/// - `Validated`: 검증 완료된 설정
#[derive(Debug, Clone)]
pub struct LogSettings<S: TypeState = Validated> {
    /// 로그 형식 (text/json)
    pub format: LogFormat,
    
    /// 로그 레벨
    pub level: Level,
    
    /// 로그 출력 대상
    pub output: LogOutput,
    
    /// 타입 상태 마커
    _marker: PhantomData<S>,
}

/// 모든 상태에 대한 공통 구현
impl<S: TypeState + 'static> LogSettings<S> {
    /// 로그 형식 반환
    pub fn format(&self) -> &LogFormat {
        &self.format
    }
    
    /// 로그 레벨 반환
    pub fn level(&self) -> Level {
        self.level
    }
    
    /// 로그 출력 설정 반환
    pub fn output(&self) -> &LogOutput {
        &self.output
    }
}

/// Raw 상태의 LogSettings에 대한 구현
impl LogSettings<Raw> {
    /// 기존 코드와의 호환성을 위한 검증 메서드
    /// 내부적으로 Validatable 트레이트의 validate를 호출
    pub fn validated(self) -> Result<LogSettings<Validated>, SettingsError> {
        self.validate()
    }

    /// 환경 변수에서 Raw 로깅 설정 생성 - 내부 공통 함수
    pub fn from_env() -> Result<Self, SettingsError> {
        create_from_env()
    }
}

/// Validated 상태의 LogSettings에 대한 구현
impl LogSettings<Validated> {
    /// 로그 출력 경로 반환 (File인 경우에만)
    pub fn output_path(&self) -> Option<&str> {
        match &self.output {
            LogOutput::File(path) => Some(path),
            _ => None,
        }
    }
}

/// 환경 변수에서 Raw 로깅 설정 생성 - 내부 공통 함수
fn create_from_env() -> Result<LogSettings<Raw>, SettingsError> {
    Ok(LogSettings {
        format: std::env::var("LOG_FORMAT")
            .map_err(|_| ())
            .and_then(|s| LogFormat::from_str(&s).map_err(|_| ()))
            .unwrap_or_default(),
        level: std::env::var("LOG_LEVEL")
            .map(parse_log_level)
            .unwrap_or_else(|_| Ok(Level::INFO))?,
        output: parse_log_output()?,
        _marker: PhantomData,
    })
}

/// `Raw` 상태에서 `Validated` 상태로의 변환을 위한 검증 구현
impl Validatable<LogSettings<Validated>> for LogSettings<Raw> {
    type Error = SettingsError;
    
    fn validate(self) -> Result<LogSettings<Validated>, SettingsError> {
        let mut validator = SettingsValidator::new();
        validator.start_collecting();
        
        // 출력이 파일인 경우 디렉터리 존재 여부 확인
        if let LogOutput::File(path) = &self.output {
            let path = Path::new(path);
            
            // 경로가 비어 있는지 확인
            if path.to_string_lossy().is_empty() {
                validator.add_error(SettingsError::MissingField {
                    field: "output".to_string(),
                    context: "로그 설정".to_string(),
                });
            }
            
            // 상위 디렉터리가 존재하는지 확인
            if let Some(parent) = path.parent() {
                if !parent.to_string_lossy().is_empty() && !parent.exists() {
                    validator.add_error(SettingsError::FileNotFound {
                        path: parent.to_string_lossy().to_string(),
                        context: "로그 파일 디렉터리".to_string(),
                    });
                }
            }
        }
        
        // 검증 오류가 있으면 반환
        if validator.has_errors() {
            return Err(validator.into_error());
        }
        
        // 검증 성공, Validated 상태로 변환
        Ok(LogSettings {
            format: self.format,
            level: self.level,
            output: self.output,
            _marker: PhantomData,
        })
    }
}

// AsyncValidatable은 제거 - 기본 구현 활용

impl Default for LogSettings<Raw> {
    fn default() -> Self {
        Self {
            format: LogFormat::default(),
            level: Level::INFO,
            output: LogOutput::default(),
            _marker: PhantomData,
        }
    }
}

impl Default for LogSettings<Validated> {
    fn default() -> Self {
        LogSettings::<Raw>::default()
            .validated()
            .expect("기본 로그 설정을 검증할 수 없습니다")
    }
}

fn parse_log_level(level: String) -> Result<Level, SettingsError> {
    Level::from_str(&level).map_err(|e| SettingsError::InvalidValue {
        field: "level".to_string(),
        context: "로그 설정".to_string(),
        message: format!("로그 레벨 파싱 오류: {}", e),
    })
}

fn parse_log_output() -> Result<LogOutput, SettingsError> {
    let output = std::env::var("LOG_OUTPUT")
        .map_err(|_| ())
        .and_then(|s| LogOutput::from_str(&s).map_err(|_| ()))
        .unwrap_or_default();
    
    Ok(output)
}

// LogSettings<Raw>에 대한 커스텀 역직렬화 구현
impl<'de> Deserialize<'de> for LogSettings<Raw> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct LogSettingsHelper {
            #[serde(default)]
            format: LogFormat,
            
            #[serde(default = "default_log_level")]
            level: String,
            
            #[serde(default)]
            output: LogOutput,
        }
        
        // 임시 구조체를 역직렬화
        let helper = LogSettingsHelper::deserialize(deserializer)?;
        
        // 로그 레벨 문자열을 파싱
        let level = match helper.level.parse::<Level>() {
            Ok(level) => level,
            Err(_) => {
                return Err(D::Error::custom(
                    format!("유효하지 않은 로그 레벨: {}", helper.level)
                ));
            }
        };
        
        Ok(LogSettings {
            format: helper.format,
            level,
            output: helper.output,
            _marker: PhantomData,
        })
    }
}

fn default_log_level() -> String {
    "INFO".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_default_log_settings_are_valid() {
        let settings = LogSettings::<Raw>::default();
        let validated = settings.validated();
        assert!(validated.is_ok());
    }
    
    #[test]
    fn test_validate_stdout_output() {
        let raw = LogSettings::<Raw> {
            format: LogFormat::Text,
            level: Level::INFO,
            output: LogOutput::Stdout,
            _marker: PhantomData,
        };
        
        let validated = raw.validate();
        assert!(validated.is_ok());
    }
    
    #[test]
    fn test_validate_file_output_nonexistent_dir() {
        let raw = LogSettings::<Raw> {
            format: LogFormat::Text,
            level: Level::INFO,
            output: LogOutput::File(String::from("/nonexistent/dir/log.txt")),
            _marker: PhantomData,
        };
        
        let validated = raw.validate();
        assert!(validated.is_err());
    }
    
    #[test]
    fn test_validate_file_output_existing_dir() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("log.txt").to_string_lossy().to_string();
        
        let raw = LogSettings::<Raw> {
            format: LogFormat::Text,
            level: Level::INFO,
            output: LogOutput::File(file_path),
            _marker: PhantomData,
        };
        
        let validated = raw.validate();
        assert!(validated.is_ok());
    }
} 