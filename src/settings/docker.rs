use serde::Deserialize;
use std::marker::PhantomData;
use super::{SettingsError, parse_env_var};
use super::typestate::{TypeState, Raw, Validated, Validatable};
use crate::settings::error::SettingsValidator;
use crate::settings::typestate::ValidationErrorCollector;

pub type Result<T> = std::result::Result<T, SettingsError>;

// 기본값 함수 정의
fn default_docker_network() -> String {
    "reverse-proxy-network".to_string()
}

fn default_label_prefix() -> String {
    "rproxy.".to_string()
}

/// HTTP 헬스 체크 기본 메서드
fn default_http_method() -> String {
    "GET".to_string()
}

fn default_http_status() -> u16 {
    200
}

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

    /// 최대 연속 실패 횟수
    #[serde(default = "default_max_failures")]
    pub max_failures: u32,
}

impl Default for HealthCheckSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: default_check_interval(),
            timeout: default_check_timeout(),
            check_type: HealthCheckType::default(),
            max_failures: default_max_failures(),
        }
    }
}

fn default_check_interval() -> u64 {
    30 // 30초
}

fn default_check_timeout() -> u64 {
    5 // 5초
}

fn default_max_failures() -> u32 {
    3  // 기본값 3회
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
pub struct LoadBalancerSettings {
    /// 로드밸런서 타입 (기본값: roundrobin)
    #[serde(default = "default_lb_strategy")]
    pub strategy: String,

    /// 가중치 (weighted 전략일 때만 사용)
    #[serde(default = "default_weight")]
    pub weight: usize,
}

impl Default for LoadBalancerSettings {
    fn default() -> Self {
        Self {
            strategy: default_lb_strategy(),
            weight: default_weight(),
        }
    }
}

fn default_lb_strategy() -> String {
    "roundrobin".to_string()
}

fn default_weight() -> usize {
    1
}

/// Docker 설정
/// 
/// 타입 매개변수 S는 설정 상태를 나타냅니다:
/// - `Raw`: 검증되지 않은 원시 설정
/// - `Validated`: 검증 완료된 설정
#[derive(Debug, Clone, Deserialize)]
pub struct DockerSettings<S: TypeState = Validated> {
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

    /// 로드밸런서 설정
    #[serde(default)]
    pub load_balancer: LoadBalancerSettings,

    /// 초기 헬스체크 설정 여부
    #[serde(default)]
    pub setup_initial_health_checks: bool,
    
    /// 타입 상태 마커
    #[serde(skip)]
    _marker: PhantomData<S>,
}

/// 모든 상태에 대한 공통 구현
impl<S: TypeState + 'static> DockerSettings<S> {
    /// Docker 네트워크 이름 반환
    pub fn network(&self) -> &str {
        &self.network
    }
    
    /// 라벨 접두사 반환
    pub fn label_prefix(&self) -> &str {
        &self.label_prefix
    }
    
    /// 헬스 체크 설정 반환
    pub fn health_check(&self) -> &HealthCheckSettings {
        &self.health_check
    }
    
    /// 재시도 설정 반환
    pub fn retry(&self) -> &RetrySettings {
        &self.retry
    }
    
    /// 로드밸런서 설정 반환
    pub fn load_balancer(&self) -> &LoadBalancerSettings {
        &self.load_balancer
    }
    
    /// 초기 헬스체크 설정 여부 반환
    pub fn setup_initial_health_checks(&self) -> bool {
        self.setup_initial_health_checks
    }
}

/// Raw 상태에 대한 구현
impl DockerSettings<Raw> {
    /// 환경 변수에서 설정 로드
    pub fn from_env() -> Result<Self> {
        let network = parse_env_var("PROXY_DOCKER_NETWORK", default_docker_network)?;
        let label_prefix = parse_env_var("PROXY_LABEL_PREFIX", default_label_prefix)?;
        let health_check = HealthCheckSettings::default();
        let retry = RetrySettings::default();
        let load_balancer = LoadBalancerSettings::default();

        let settings = Self {
            network,
            label_prefix,
            health_check,
            retry,
            load_balancer,
            setup_initial_health_checks: false,
            _marker: PhantomData,
        };
        
        Ok(settings)
    }
    
    /// 설정 검증하고 Validated 상태로 변환
    pub fn validated(self) -> Result<DockerSettings<Validated>> {
        self.validate()
    }
    
    /// 비동기 설정 검증하고 Validated 상태로 변환
    pub async fn validated_async(self) -> Result<DockerSettings<Validated>> {
        // Docker 설정은 파일 시스템 접근이 필요 없으므로 동기 메서드 호출
        self.validate()
    }
}

impl Validatable<DockerSettings<Validated>> for DockerSettings<Raw> {
    type Error = SettingsError;
    
    fn validate(self) -> Result<DockerSettings<Validated>> {
        let mut validator = SettingsValidator::new();
        validator.start_collecting();
        
        // 빈 네트워크 이름 검사
        if self.network.is_empty() {
            validator.add_error(SettingsError::MissingField {
                field: "network".to_string(),
                context: "Docker 설정".to_string(),
            });
        }

        // 라벨 접두사 길이 제한
        if self.label_prefix.len() > 100 {
            validator.add_error(SettingsError::InvalidValue {
                field: "label_prefix".to_string(),
                context: "Docker 설정".to_string(),
                message: "라벨 접두사가 너무 깁니다 (최대 100자)".to_string(),
            });
        }

        // Docker 네트워크 이름 검증
        if !self.network.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            validator.add_error(SettingsError::InvalidValue {
                field: "network".to_string(),
                context: "Docker 설정".to_string(),
                message: "Docker 네트워크 이름은 영숫자와 -_ 만 포함할 수 있습니다".to_string(),
            });
        }

        // 라벨 접두사 검증
        if !self.label_prefix.ends_with('.') {
            validator.add_error(SettingsError::InvalidValue {
                field: "label_prefix".to_string(),
                context: "Docker 설정".to_string(),
                message: "라벨 접두사는 '.'으로 끝나야 합니다".to_string(),
            });
        }
        
        if validator.has_errors() {
            return Err(validator.into_error());
        }
        
        // 검증 성공, Validated 상태로 변환
        Ok(DockerSettings {
            network: self.network,
            label_prefix: self.label_prefix,
            health_check: self.health_check,
            retry: self.retry,
            load_balancer: self.load_balancer,
            setup_initial_health_checks: self.setup_initial_health_checks,
            _marker: PhantomData,
        })
    }
}

/// 검증된 상태에 대한 Default 구현
impl Default for DockerSettings<Raw> {
    fn default() -> Self {
        Self {
            network: default_docker_network(),
            label_prefix: default_label_prefix(),
            health_check: HealthCheckSettings::default(),
            retry: RetrySettings::default(),
            load_balancer: LoadBalancerSettings::default(),
            setup_initial_health_checks: false,
            _marker: PhantomData,
        }
    }
}

/// 검증된 상태에 대한 Default 구현
impl Default for DockerSettings<Validated> {
    fn default() -> Self {
        // Raw 상태의 기본값을 생성하고 검증
        DockerSettings::<Raw>::default()
            .validate()
            .expect("기본 Docker 설정은 항상 유효해야 합니다")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_valid_docker_settings() {
        let settings = DockerSettings::<Raw> {
            network: "test-network".to_string(),
            label_prefix: "test.".to_string(),
            health_check: HealthCheckSettings::default(),
            retry: RetrySettings::default(),
            load_balancer: LoadBalancerSettings::default(),
            setup_initial_health_checks: false,
            _marker: PhantomData,
        };
        
        let result = settings.validate();
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_invalid_network_name() {
        let settings = DockerSettings::<Raw> {
            network: "invalid@network".to_string(),
            label_prefix: "test.".to_string(),
            health_check: HealthCheckSettings::default(),
            retry: RetrySettings::default(),
            load_balancer: LoadBalancerSettings::default(),
            setup_initial_health_checks: false,
            _marker: PhantomData,
        };
        
        let result = settings.validate();
        assert!(result.is_err());
    }
    
    #[test]
    fn test_invalid_label_prefix() {
        let settings = DockerSettings::<Raw> {
            network: "test-network".to_string(),
            label_prefix: "test".to_string(), // 마침표가 없음
            health_check: HealthCheckSettings::default(),
            retry: RetrySettings::default(),
            load_balancer: LoadBalancerSettings::default(),
            setup_initial_health_checks: false,
            _marker: PhantomData,
        };
        
        let result = settings.validate();
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_validated_async() {
        let settings = DockerSettings::<Raw> {
            network: "test-network".to_string(),
            label_prefix: "test.".to_string(),
            health_check: HealthCheckSettings::default(),
            retry: RetrySettings::default(),
            load_balancer: LoadBalancerSettings::default(),
            setup_initial_health_checks: false,
            _marker: PhantomData,
        };
        
        let result = settings.validated_async().await;
        assert!(result.is_ok());
    }
} 