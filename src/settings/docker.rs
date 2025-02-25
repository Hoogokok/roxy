use serde::Deserialize;
use super::{SettingsError, parse_env_var};
pub type Result<T> = std::result::Result<T, SettingsError>;

/// 헬스 체크 타입
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthCheckType {
    /// HTTP 헬스 체크
    Http {
        /// 체크할 경로 (예: /health)
        path: String,
        /// HTTP 메서드 (기본값: GET)
        #[serde(default = "default_http_method")]
        method: String,
        /// 정상 상태 코드 (기본값: 200)
        #[serde(default = "default_http_status")]
        expected_status: u16,
    },
    /// TCP 포트 체크
    Tcp {
        /// 체크할 포트
        port: u16,
    },
}

impl Default for HealthCheckType {
    fn default() -> Self {
        Self::Http {
            path: "/health".to_string(),
            method: default_http_method(),
            expected_status: default_http_status(),
        }
    }
}

fn default_http_method() -> String {
    "GET".to_string()
}

fn default_http_status() -> u16 {
    200
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthCheckSettings {
    /// 헬스 체크 활성화 여부
    #[serde(default)]
    pub enabled: bool,

    /// 체크 간격 (초)
    #[serde(default = "default_check_interval")]
    pub interval: u64,

    /// 체크 타임아웃 (초)
    #[serde(default = "default_check_timeout")]
    pub timeout: u64,

    /// 헬스 체크 타입
    #[serde(default)]
    pub check_type: HealthCheckType,
}

impl Default for HealthCheckSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: default_check_interval(),
            timeout: default_check_timeout(),
            check_type: HealthCheckType::default(),
        }
    }
}

fn default_check_interval() -> u64 {
    30 // 30초
}

fn default_check_timeout() -> u64 {
    5 // 5초
}

/// 재시도 설정
#[derive(Debug, Clone, Deserialize)]
pub struct RetrySettings {
    /// 최대 재시도 횟수
    #[serde(default = "default_retry_attempts")]
    pub max_attempts: u32,

    /// 재시도 간격 (초)
    #[serde(default = "default_retry_interval")]
    pub interval: u64,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            max_attempts: default_retry_attempts(),
            interval: default_retry_interval(),
        }
    }
}

fn default_retry_attempts() -> u32 {
    3
}

fn default_retry_interval() -> u64 {
    1 // 1초
}

#[derive(Debug, Clone, Deserialize)]
pub struct DockerSettings {
    /// Docker 네트워크 이름
    #[serde(default = "default_docker_network")]
    pub network: String,

    /// 라벨 접두사
    #[serde(default = "default_label_prefix")]
    pub label_prefix: String,

    /// 헬스 체크 설정
    #[serde(default)]
    pub health_check: HealthCheckSettings,

    /// 재시도 설정
    #[serde(default)]
    pub retry: RetrySettings,
}

impl DockerSettings {
    pub fn from_env() -> Result<Self> {
        let network = parse_env_var("PROXY_DOCKER_NETWORK", default_docker_network)?;
        let label_prefix = parse_env_var("PROXY_LABEL_PREFIX", default_label_prefix)?;
        let health_check = HealthCheckSettings::default();
        let retry = RetrySettings::default();

        let settings = Self {
            network,
            label_prefix,
            health_check,
            retry,
        };
        settings.validate()?;
        Ok(settings)
    }

   pub fn validate(&self) -> Result<()> {
        // 빈 네트워크 이름 검사
        if self.network.is_empty() {
            return Err(SettingsError::EnvVarInvalid {
                var_name: "PROXY_DOCKER_NETWORK".to_string(),
                value: self.network.clone(),
                reason: "네트워크 이름은 비어있을 수 없습니다".to_string(),
            });
        }

        // 라벨 접두사 길이 제한
        if self.label_prefix.len() > 100 {
            return Err(SettingsError::EnvVarInvalid {
                var_name: "PROXY_LABEL_PREFIX".to_string(),
                value: self.label_prefix.clone(),
                reason: "라벨 접두사가 너무 깁니다 (최대 100자)".to_string(),
            });
        }

        // Docker 네트워크 이름 검증
        if !self.network.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(SettingsError::EnvVarInvalid {
                var_name: "PROXY_DOCKER_NETWORK".to_string(),
                value: self.network.clone(),
                reason: "Docker 네트워크 이름은 영숫자와 -_ 만 포함할 수 있습니다".to_string(),
            });
        }

        // 라벨 접두사 검증
        if !self.label_prefix.ends_with('.') {
            return Err(SettingsError::EnvVarInvalid {
                var_name: "PROXY_LABEL_PREFIX".to_string(),
                value: self.label_prefix.clone(),
                reason: "라벨 접두사는 '.'으로 끝나야 합니다".to_string(),
            });
        }

        Ok(())
    }
}

impl Default for DockerSettings {
    fn default() -> Self {
        Self {
            network: default_docker_network(),
            label_prefix: default_label_prefix(),
            health_check: HealthCheckSettings::default(),
            retry: RetrySettings::default(),
        }
    }
}

fn default_docker_network() -> String {
    "reverse-proxy-network".to_string()
}

fn default_label_prefix() -> String {
    "rproxy.".to_string()
} 