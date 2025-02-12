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

    /// Docker 네트워크 이름
    #[serde(default = "default_docker_network")]
    pub docker_network: String,

    /// Docker 라벨 접두사
    #[serde(default = "default_label_prefix")]
    pub label_prefix: String,
}

fn default_http_port() -> u16 { 8080 }
fn default_https_port() -> u16 { 443 }
fn default_docker_network() -> String { "reverse-proxy-network".to_string() }
fn default_label_prefix() -> String { "rproxy.".to_string() }

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
    pub fn from_env() -> Result<Self, SettingsError> {
        Ok(Self {
            http_port: parse_env_var::<u16, _>("PROXY_HTTP_PORT", default_http_port)?,
            https_enabled: parse_env_var::<bool, _>("PROXY_HTTPS_ENABLED", default_https_disabled)?,
            https_port: parse_env_var::<u16, _>("PROXY_HTTPS_PORT", default_https_port)?,
            tls_cert_path: env::var("PROXY_TLS_CERT").ok(),
            tls_key_path: env::var("PROXY_TLS_KEY").ok(),
            docker_network: parse_env_var::<String, _>("PROXY_DOCKER_NETWORK", default_docker_network)?,
            label_prefix: parse_env_var::<String, _>("PROXY_LABEL_PREFIX", default_label_prefix)?,
        })
    }

    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.http_port == 0 || self.http_port > 65535 {
            return Err(SettingsError::EnvVarInvalid {
                var_name: "PROXY_HTTP_PORT".to_string(),
                value: self.http_port.to_string(),
                reason: "포트는 1-65535 범위여야 합니다".to_string(),
            });
        }
        
        if self.https_enabled && (self.https_port == 0 || self.https_port > 65535) {
            return Err(SettingsError::EnvVarInvalid {
                var_name: "PROXY_HTTPS_PORT".to_string(),
                value: self.https_port.to_string(),
                reason: "포트는 1-65535 범위여야 합니다".to_string(),
            });
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
            docker_network: default_docker_network(),
            label_prefix: default_label_prefix(),
        }
    }
} 