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

impl ServerSettings {
    pub fn from_env() -> Result<Self, SettingsError> {
        Ok(Self {
            http_port: parse_env_var("PROXY_HTTP_PORT", default_http_port)?,
            https_enabled: parse_env_var("PROXY_HTTPS_ENABLED", || false)?,
            https_port: parse_env_var("PROXY_HTTPS_PORT", default_https_port)?,
            tls_cert_path: env::var("PROXY_TLS_CERT").ok(),
            tls_key_path: env::var("PROXY_TLS_KEY").ok(),
            docker_network: parse_env_var("PROXY_DOCKER_NETWORK", default_docker_network)?,
            label_prefix: parse_env_var("PROXY_LABEL_PREFIX", default_label_prefix)?,
        })
    }
}

fn default_http_port() -> u16 { 8080 }
fn default_https_port() -> u16 { 443 }
fn default_docker_network() -> String { "reverse-proxy-network".to_string() }
fn default_label_prefix() -> String { "rproxy.".to_string() }

fn parse_env_var<T, F>(name: &str, default: F) -> Result<T, SettingsError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
    F: FnOnce() -> T,
{
    match env::var(name) {
        Ok(val) => val.parse().map_err(|e| SettingsError::EnvVarInvalid {
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