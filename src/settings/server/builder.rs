use std::env;

use crate::settings::error::SettingsError;
use crate::settings::server::settings::{ServerSettings, default_http_port, default_https_port};
use crate::settings::server::types::{HttpsEnabled, HttpsDisabled};
use crate::settings::types::ValidPort;
use crate::settings::typestate::Raw;

/// 환경 변수에서 값을 파싱하는 유틸리티 함수
pub fn parse_env_var<T: std::str::FromStr, F: FnOnce() -> T>(name: &str, default: F) -> Result<T, SettingsError>
where
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(val) => match val.parse::<T>() {
            Ok(parsed) => Ok(parsed),
            Err(e) => Err(SettingsError::EnvVarInvalid {
                var_name: name.to_string(),
                value: val,
                reason: e.to_string(),
            }),
        },
        Err(env::VarError::NotPresent) => Ok(default()),
        Err(env::VarError::NotUnicode(_)) => Err(SettingsError::EnvVarInvalid {
            var_name: name.to_string(),
            value: "Not Unicode".to_string(),
            reason: "환경 변수 값이 유효한 유니코드가 아닙니다.".to_string(),
        }),
    }
}

/// 서버 설정을 생성하기 위한 빌더 패턴 구현
pub struct ServerBuilder {
    http_port: ValidPort,
    https_port: Option<ValidPort>,
    tls_cert_path: Option<String>,
    tls_key_path: Option<String>,
}

impl ServerBuilder {
    /// 지정된 HTTP 포트로 빌더 생성
    pub fn new(http_port: ValidPort) -> Self {
        Self {
            http_port,
            https_port: None,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
    
    /// 기본 설정으로 빌더 생성
    pub fn default() -> Self {
        Self::new(default_http_port())
    }
    
    /// HTTPS 포트 설정
    pub fn with_https_port(mut self, https_port: ValidPort) -> Self {
        self.https_port = Some(https_port);
        self
    }
    
    /// TLS 인증서 경로 설정
    pub fn with_tls_cert_path(mut self, cert_path: String) -> Self {
        self.tls_cert_path = Some(cert_path);
        self
    }
    
    /// TLS 키 경로 설정
    pub fn with_tls_key_path(mut self, key_path: String) -> Self {
        self.tls_key_path = Some(key_path);
        self
    }
    
    /// HTTP 전용 서버 설정 생성
    pub fn build_http(self) -> ServerSettings<Raw, HttpsDisabled> {
        ServerSettings::create(
            self.http_port,
            self.https_port.unwrap_or_else(default_https_port),
            None,
            None,
        )
    }
    
    /// HTTPS 지원 서버 설정 생성
    pub fn build_https(self) -> Result<ServerSettings<Raw, HttpsEnabled>, SettingsError> {
        // HTTPS 포트가 설정되지 않은 경우
        let https_port = match self.https_port {
            Some(port) => port,
            None => {
                return Err(SettingsError::MissingField {
                    field: "https_port".into(),
                    context: "HTTPS가 활성화된 경우 HTTPS 포트는 필수입니다.".into(),
                });
            }
        };
        
        // 인증서 경로가 없는 경우
        let cert_path = match self.tls_cert_path {
            Some(path) => path,
            None => {
                return Err(SettingsError::MissingField {
                    field: "tls_cert_path".into(),
                    context: "HTTPS가 활성화된 경우 인증서 경로는 필수입니다.".into(),
                });
            }
        };
        
        // 키 경로가 없는 경우
        let key_path = match self.tls_key_path {
            Some(path) => path,
            None => {
                return Err(SettingsError::MissingField {
                    field: "tls_key_path".into(),
                    context: "HTTPS가 활성화된 경우 키 경로는 필수입니다.".into(),
                });
            }
        };
        
        Ok(ServerSettings::create(
            self.http_port,
            https_port,
            Some(cert_path),
            Some(key_path),
        ))
    }
    
    /// 환경 변수에서 설정 로드
    pub fn from_env() -> Result<Self, SettingsError> {
        // HTTP 포트 로드
        let http_port = parse_env_var("HTTP_PORT", || default_http_port().value())?;
        let http_port = match ValidPort::new(http_port) {
            Some(port) => port,
            None => {
                return Err(SettingsError::InvalidValue {
                    field: "HTTP_PORT".into(),
                    context: "HTTP 포트 검증 실패".into(),
                    message: format!("HTTP_PORT 값 {}이(가) 유효한 포트 범위가 아닙니다 (1-65535).", http_port),
                });
            }
        };
        
        // HTTPS 활성화 여부 확인
        let https_enabled = parse_env_var("HTTPS_ENABLED", || false)?;
        
        let mut builder = Self::new(http_port);
        
        // HTTPS가 활성화된 경우 추가 설정 로드
        if https_enabled {
            // HTTPS 포트 로드
            let https_port = parse_env_var("HTTPS_PORT", || default_https_port().value())?;
            let https_port = match ValidPort::new(https_port) {
                Some(port) => port,
                None => {
                    return Err(SettingsError::InvalidValue {
                        field: "HTTPS_PORT".into(),
                        context: "HTTPS 포트 검증 실패".into(),
                        message: format!("HTTPS_PORT 값 {}이(가) 유효한 포트 범위가 아닙니다 (1-65535).", https_port),
                    });
                }
            };
            
            builder = builder.with_https_port(https_port);
            
            // 인증서 경로 로드
            match env::var("TLS_CERT_PATH") {
                Ok(path) => {
                    builder = builder.with_tls_cert_path(path);
                }
                Err(env::VarError::NotPresent) => {
                    return Err(SettingsError::EnvVarMissing {
                        var_name: "TLS_CERT_PATH".into(),
                    });
                }
                Err(env::VarError::NotUnicode(_)) => {
                    return Err(SettingsError::EnvVarInvalid {
                        var_name: "TLS_CERT_PATH".into(),
                        value: "Not Unicode".into(),
                        reason: "환경 변수 값이 유효한 유니코드가 아닙니다.".into(),
                    });
                }
            }
            
            // 키 경로 로드
            match env::var("TLS_KEY_PATH") {
                Ok(path) => {
                    builder = builder.with_tls_key_path(path);
                }
                Err(env::VarError::NotPresent) => {
                    return Err(SettingsError::EnvVarMissing {
                        var_name: "TLS_KEY_PATH".into(),
                    });
                }
                Err(env::VarError::NotUnicode(_)) => {
                    return Err(SettingsError::EnvVarInvalid {
                        var_name: "TLS_KEY_PATH".into(),
                        value: "Not Unicode".into(),
                        reason: "환경 변수 값이 유효한 유니코드가 아닙니다.".into(),
                    });
                }
            }
        }
        
        Ok(builder)
    }
} 