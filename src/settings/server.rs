use serde::Deserialize;
use std::env;
use super::SettingsError;

#[derive(Clone, Debug, Deserialize)]
pub struct ServerSettings {
    /// HTTP 포트 (기본값: 8080)
    #[serde(default = "default_http_port")]
    pub http_port: u16,

    /// HTTPS 활성화 여부
    #[serde(default)]
    pub https_enabled: bool,

    /// HTTPS 포트 (기본값: 443)
    #[serde(default = "default_https_port")]
    pub https_port: u16,

    /// TLS 인증서 경로
    pub tls_cert_path: Option<String>,

    /// TLS 키 경로
    pub tls_key_path: Option<String>,
}

fn default_http_port() -> u16 { 80 }
fn default_https_port() -> u16 { 443 }

fn default_https_disabled() -> bool { false }

pub fn parse_env_var<T: std::str::FromStr, F: FnOnce() -> T>(name: &str, default: F) -> Result<T, SettingsError>
where
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(val) => val.parse().map_err(|e: T::Err| SettingsError::EnvVarInvalid {
            var_name: name.to_string(),
            value: val,
            reason: e.to_string(),
        }),
        Err(env::VarError::NotPresent) => Ok(default()),
        Err(e) => Err(SettingsError::EnvVarInvalid {
            var_name: name.to_string(),
            value: "".to_string(),
            reason: e.to_string(),
        }),
    }
}

impl ServerSettings {
    // 포트 범위 상수 추가
    const MIN_PORT: u16 = 1;
    const MAX_PORT: u16 = 65535;
    
    // 포트 파싱 전용 함수
    fn parse_port(name: &str, value: &str) -> Result<u16, SettingsError> {
        let port = value.parse::<u16>().map_err(|_| SettingsError::EnvVarInvalid {
            var_name: name.to_string(),
            value: value.to_string(),
            reason: format!("포트는 {}-{} 범위여야 합니다", Self::MIN_PORT, Self::MAX_PORT),
        })?;
        
        if port < Self::MIN_PORT {
            return Err(SettingsError::EnvVarInvalid {
                var_name: name.to_string(),
                value: value.to_string(),
                reason: "포트는 0이 될 수 없습니다".to_string(),
            });
        }
        
        Ok(port)
    }

    pub fn from_env() -> Result<Self, SettingsError> {
        let http_port = Self::parse_port(
            "PROXY_HTTP_PORT",
            &env::var("PROXY_HTTP_PORT").unwrap_or_else(|_| default_http_port().to_string())
        )?;

        let https_port = Self::parse_port(
            "PROXY_HTTPS_PORT",
            &env::var("PROXY_HTTPS_PORT").unwrap_or_else(|_| default_https_port().to_string())
        )?;

        let settings = Self {
            http_port,
            https_port,
            https_enabled: parse_env_var::<bool, _>("PROXY_HTTPS_ENABLED", default_https_disabled)?,
            tls_cert_path: env::var("PROXY_TLS_CERT").ok(),
            tls_key_path: env::var("PROXY_TLS_KEY").ok(),
        };
        
        settings.validate()?;
        Ok(settings)
    }

    pub fn validate(&self) -> Result<(), SettingsError> {
        // HTTPS가 활성화된 경우 인증서/키 파일 필수 검사
        if self.https_enabled {
            if self.tls_cert_path.is_none() {
                return Err(SettingsError::EnvVarMissing {
                    var_name: "PROXY_TLS_CERT".to_string()
                });
            }
            if self.tls_key_path.is_none() {
                return Err(SettingsError::EnvVarMissing {
                    var_name: "PROXY_TLS_KEY".to_string()
                });
            }

            // HTTP/HTTPS 포트 충돌 검사
            if self.http_port == self.https_port {
                return Err(SettingsError::EnvVarInvalid {
                    var_name: "PROXY_HTTP_PORT/PROXY_HTTPS_PORT".to_string(),
                    value: format!("{}/{}", self.http_port, self.https_port),
                    reason: "HTTP와 HTTPS 포트는 달라야 합니다".to_string(),
                });
            }
        }

        Ok(())
    }
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            http_port: default_http_port(),
            https_enabled: false,
            https_port: default_https_port(),
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
} 