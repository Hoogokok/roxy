use std::{env, path::PathBuf, marker::PhantomData};
use serde::Deserialize;
use tokio::fs;
use super::{SettingsError, parse_env_var};
use crate::settings::error::SettingsValidator;
use crate::settings::typestate::{Raw, Validated, TypeState, AsyncValidatable, ValidationErrorCollector};

#[derive(Debug, Clone)]
pub struct TlsSettings<State: TypeState = Validated> {
    /// HTTPS 활성화 여부
    pub enabled: bool,

    /// HTTPS 포트
    pub port: u16,

    /// 인증서 파일 경로
    pub cert_path: Option<PathBuf>,

    /// 개인키 파일 경로
    pub key_path: Option<PathBuf>,
    
    /// 상태 마커
    _marker: PhantomData<State>,
}

// TlsSettings<Raw>에 대한 커스텀 역직렬화 구현
impl<'de> Deserialize<'de> for TlsSettings<Raw> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // 임시 구조체로 역직렬화
        #[derive(Deserialize)]
        struct TlsHelper {
            #[serde(default)]
            enabled: bool,
            
            #[serde(default = "default_https_port")]
            port: u16,
            
            cert_path: Option<PathBuf>,
            key_path: Option<PathBuf>,
        }
        
        let helper = TlsHelper::deserialize(deserializer)?;
        
        Ok(TlsSettings {
            enabled: helper.enabled,
            port: helper.port,
            cert_path: helper.cert_path,
            key_path: helper.key_path,
            _marker: PhantomData,
        })
    }
}

// Raw 상태에서의 메서드
impl TlsSettings<Raw> {
    pub fn new(
        enabled: bool,
        port: u16,
        cert_path: Option<PathBuf>,
        key_path: Option<PathBuf>,
    ) -> Self {
        Self {
            enabled,
            port,
            cert_path,
            key_path,
            _marker: PhantomData,
        }
    }
    
    pub fn from_env() -> Result<Self, SettingsError> {
        Ok(Self {
            enabled: parse_env_var("PROXY_TLS_ENABLED", || false)?,
            port: parse_env_var("PROXY_TLS_PORT", default_https_port)?,
            cert_path: env::var("PROXY_TLS_CERT")
                .map(PathBuf::from)
                .ok(),
            key_path: env::var("PROXY_TLS_KEY")
                .map(PathBuf::from)
                .ok(),
            _marker: PhantomData,
        })
    }
    
    /// 유효성 검사를 수행하고 Validated 상태로 전환
    pub async fn validated(self) -> Result<TlsSettings<Validated>, SettingsError> {
        // 내부적으로 AsyncValidatable 트레이트를 활용
        self.validate_async().await
    }
}

/// AsyncValidatable 트레이트 구현 - 비동기 검증 수행
impl AsyncValidatable<TlsSettings<Validated>> for TlsSettings<Raw> {
    type Error = SettingsError;
    
    async fn validate_async(self) -> Result<TlsSettings<Validated>, Self::Error> {
        let mut validator = SettingsValidator::new();
        validator.start_collecting();
        
        // TLS가 활성화된 경우에만 인증서와 키 파일 검증
        if self.enabled {
            // 인증서 파일 존재 여부 확인
            if let Some(cert_path) = &self.cert_path {
                if !cert_path.exists() {
                    let err = SettingsError::FileNotFound {
                        path: cert_path.to_string_lossy().to_string(),
                        context: "TLS 인증서".to_string(),
                    };
                    validator.add_error(err);
                } else {
                    // 파일 읽기 권한 확인
                    match fs::metadata(cert_path).await {
                        Ok(_) => {}, // 메타데이터 접근 가능
                        Err(e) => {
                            let err = SettingsError::FileAccessError {
                                path: cert_path.to_string_lossy().to_string(),
                                context: "TLS 인증서".to_string(),
                                error: e.to_string(),
                            };
                            validator.add_error(err);
                        }
                    }
                }
            } else {
                let err = SettingsError::MissingField {
                    field: "cert_path".to_string(),
                    context: "TLS 설정".to_string(),
                };
                validator.add_error(err);
            }
            
            // 키 파일 존재 여부 확인
            if let Some(key_path) = &self.key_path {
                if !key_path.exists() {
                    let err = SettingsError::FileNotFound {
                        path: key_path.to_string_lossy().to_string(),
                        context: "TLS 개인키".to_string(),
                    };
                    validator.add_error(err);
                } else {
                    // 파일 읽기 권한 확인
                    match fs::metadata(key_path).await {
                        Ok(_) => {}, // 메타데이터 접근 가능
                        Err(e) => {
                            let err = SettingsError::FileAccessError {
                                path: key_path.to_string_lossy().to_string(),
                                context: "TLS 개인키".to_string(),
                                error: e.to_string(),
                            };
                            validator.add_error(err);
                        }
                    }
                }
            } else {
                let err = SettingsError::MissingField {
                    field: "key_path".to_string(),
                    context: "TLS 설정".to_string(),
                };
                validator.add_error(err);
            }
            
            // 포트 범위 검증
            if self.port < 1 || self.port > 65535 {
                let err = SettingsError::InvalidValue {
                    field: "port".to_string(),
                    context: "TLS 설정".to_string(),
                    message: format!("포트 범위는 1-65535여야 합니다. 현재 값: {}", self.port),
                };
                validator.add_error(err);
            }
        }
        
        // 오류가 있는 경우 처리
        if validator.has_errors() {
            return Err(validator.into_error());
        }
        
        // 검증 완료 후 Validated 상태로 변환
        Ok(TlsSettings {
            enabled: self.enabled,
            port: self.port,
            cert_path: self.cert_path,
            key_path: self.key_path,
            _marker: PhantomData,
        })
    }
}

// Validated 상태에서의 메서드들
impl TlsSettings<Validated> {
    // 안전한 게터 메서드
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    pub fn port(&self) -> u16 {
        self.port
    }
    
    pub fn cert_path(&self) -> Option<&PathBuf> {
        self.cert_path.as_ref()
    }
    
    pub fn key_path(&self) -> Option<&PathBuf> {
        self.key_path.as_ref()
    }
}

impl Default for TlsSettings<Raw> {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_https_port(),
            cert_path: None,
            key_path: None,
            _marker: PhantomData,
        }
    }
}

impl Default for TlsSettings<Validated> {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_https_port(),
            cert_path: None,
            key_path: None,
            _marker: PhantomData,
        }
    }
}

fn default_https_port() -> u16 {
    443
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::io::Write;
    use std::fs::File;
    
    #[tokio::test]
    async fn test_tls_settings_validation() {
        // 테스트용 임시 파일 생성
        let temp_dir = tempdir().unwrap();
        let cert_path = temp_dir.path().join("cert.pem");
        let key_path = temp_dir.path().join("key.pem");
        
        // 임시 인증서 및 키 파일 생성
        File::create(&cert_path).unwrap().write_all(b"TEST CERT").unwrap();
        File::create(&key_path).unwrap().write_all(b"TEST KEY").unwrap();
        
        // Raw 버전 테스트
        let settings = TlsSettings::<Raw> {
            enabled: true,
            port: 443,
            cert_path: Some(cert_path.clone()),
            key_path: Some(key_path.clone()),
            _marker: PhantomData,
        };
        
        // 유효성 검사 수행
        let validated = settings.validated().await.unwrap();
        assert!(validated.is_enabled());
        assert_eq!(validated.port(), 443);
        assert_eq!(validated.cert_path().unwrap(), &cert_path);
        assert_eq!(validated.key_path().unwrap(), &key_path);
        
        // 잘못된 경로로 검증 실패 테스트
        let invalid_settings = TlsSettings::<Raw> {
            enabled: true,
            port: 443,
            cert_path: Some(PathBuf::from("/not/exists/cert.pem")),
            key_path: Some(PathBuf::from("/not/exists/key.pem")),
            _marker: PhantomData,
        };
        
        assert!(invalid_settings.validated().await.is_err());
        
        // TLS 비활성화 상태 테스트
        let disabled_settings = TlsSettings::<Raw> {
            enabled: false,
            port: 443,
            cert_path: None,
            key_path: None,
            _marker: PhantomData,
        };
        
        // 비활성화 상태에서는 인증서/키 파일 검증 스킵
        let disabled_validated = disabled_settings.validated().await.unwrap();
        assert!(!disabled_validated.is_enabled());
        assert_eq!(disabled_validated.port(), 443);
        assert!(disabled_validated.cert_path().is_none());
        assert!(disabled_validated.key_path().is_none());
    }
} 