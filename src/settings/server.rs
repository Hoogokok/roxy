use serde::{Deserialize, Serialize};
use std::env;
use std::marker::PhantomData;
use super::SettingsError;
use super::types::ValidPort;
use std::any::TypeId;

// 기존 상태 타입
#[derive(Debug, Clone, Copy)]
pub struct Raw;

#[derive(Debug, Clone, Copy)]
pub struct Validated;

// HTTPS 활성화 상태를 표현하는 타입
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpsEnabled;

/// HTTPS 비활성화 상태를 나타내는 타입
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpsDisabled;

// HttpsState 타입 매개변수가 추가된 ServerSettings
#[derive(Clone, Debug)]
pub struct ServerSettings<State = Validated, HttpsState = HttpsDisabled> {
    /// HTTP 포트
    http_port: ValidPort,

    /// HTTPS 포트 (HttpsEnabled 상태인 경우에만 의미 있음)
    https_port: ValidPort,

    /// TLS 인증서 경로 (HttpsEnabled 상태인 경우에만 의미 있음)
    tls_cert_path: Option<String>,

    /// TLS 키 경로 (HttpsEnabled 상태인 경우에만 의미 있음)
    tls_key_path: Option<String>,
    
    /// 상태를 추적하기 위한 phantom data
    _marker: PhantomData<(State, HttpsState)>,
}

fn default_http_port() -> ValidPort { 
    ValidPort::new(80).expect("기본 HTTP 포트는 항상 유효해야 합니다") 
}

fn default_https_port() -> ValidPort { 
    ValidPort::new(443).expect("기본 HTTPS 포트는 항상 유효해야 합니다") 
}

pub fn parse_env_var<T: std::str::FromStr, F: FnOnce() -> T>(name: &str, default: F) -> Result<T, SettingsError>
where
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(val) => {
            match val.parse::<T>() {
                Ok(parsed) => Ok(parsed),
                Err(e) => Err(SettingsError::EnvVarInvalid {
                    var_name: name.to_string(),
                    reason: format!("{}", e),
                    value: val
                })
            }
        },
        Err(_) => Ok(default())
    }
}

// 공통 메서드 (모든 상태 조합에 적용)
impl<State, HttpsState> ServerSettings<State, HttpsState> {
    /// HTTP 포트 getter
    pub fn http_port(&self) -> u16 {
        self.http_port.value()
    }
    
    /// TLS 인증서 경로 getter
    pub fn tls_cert_path(&self) -> Option<&String> {
        self.tls_cert_path.as_ref()
    }
    
    /// TLS 키 경로 getter
    pub fn tls_key_path(&self) -> Option<&String> {
        self.tls_key_path.as_ref()
    }
}

// HttpsDisabled 상태 메서드
impl<State> ServerSettings<State, HttpsDisabled> {
    /// HTTPS 활성화 여부 getter (항상 false)
    pub fn https_enabled(&self) -> bool {
        false
    }
    
    /// HTTPS 포트 getter (유효하지 않음)
    pub fn https_port(&self) -> u16 {
        // HttpsDisabled 상태에서는 의미 없지만 기존 API 호환성을 위해 유지
        self.https_port.value()
    }
}

// HttpsEnabled 상태 메서드
impl<State> ServerSettings<State, HttpsEnabled> {
    /// HTTPS 활성화 여부 getter (항상 true)
    pub fn https_enabled(&self) -> bool {
        true
    }
    
    /// HTTPS 포트 getter
    pub fn https_port(&self) -> u16 {
        self.https_port.value()
    }
}

// 새로 추가: ServerBuilder 패턴
pub struct ServerBuilder {
    http_port: ValidPort,
    https_port: Option<ValidPort>,
    tls_cert_path: Option<String>,
    tls_key_path: Option<String>,
}

impl ServerBuilder {
    /// 새 서버 빌더 생성
    pub fn new(http_port: ValidPort) -> Self {
        Self {
            http_port,
            https_port: None,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
    
    /// 기본 HTTP 포트로 새 서버 빌더 생성
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
    
    /// HTTP 서버 설정 빌드 (HTTPS 비활성화)
    pub fn build_http(self) -> ServerSettings<Raw, HttpsDisabled> {
        ServerSettings {
            http_port: self.http_port,
            https_port: default_https_port(), // 기본값 사용
            tls_cert_path: None,
            tls_key_path: None,
            _marker: PhantomData,
        }
    }
    
    /// HTTPS 서버 설정 빌드 (HTTPS 활성화)
    pub fn build_https(self) -> Result<ServerSettings<Raw, HttpsEnabled>, SettingsError> {
        let https_port = self.https_port.ok_or_else(|| 
            SettingsError::ValidationError {
                field: "https_port".to_string(),
                message: "HTTPS 포트가 지정되지 않았습니다".to_string()
            }
        )?;
        
        let tls_cert_path = self.tls_cert_path.ok_or_else(|| 
            SettingsError::ValidationError {
                field: "tls_cert_path".to_string(),
                message: "HTTPS가 활성화된 경우 TLS 인증서 경로가 필요합니다".to_string()
            }
        )?;
        
        let tls_key_path = self.tls_key_path.ok_or_else(|| 
            SettingsError::ValidationError {
                field: "tls_key_path".to_string(),
                message: "HTTPS가 활성화된 경우 TLS 키 경로가 필요합니다".to_string()
            }
        )?;
        
        // 포트 충돌 검사
        if self.http_port.value() == https_port.value() {
            return Err(SettingsError::ValidationError {
                field: "https_port".to_string(),
                message: "HTTP와 HTTPS 포트는 달라야 합니다".to_string()
            });
        }
        
        Ok(ServerSettings {
            http_port: self.http_port,
            https_port,
            tls_cert_path: Some(tls_cert_path),
            tls_key_path: Some(tls_key_path),
            _marker: PhantomData,
        })
    }
    
    /// 환경 변수에서 서버 설정을 로드
    pub fn from_env() -> Result<Self, SettingsError> {
        // HTTP 포트
        let http_port_raw = parse_env_var::<u16, _>("PROXY_HTTP_PORT", || 80)?;
        let http_port = ValidPort::new(http_port_raw).ok_or_else(|| 
            SettingsError::ValidationError {
                field: "http_port".to_string(),
                message: format!("HTTP 포트는 0이 될 수 없습니다: {}", http_port_raw)
            }
        )?;
        
        let mut builder = Self::new(http_port);
        
        // HTTPS 활성화 여부
        let https_enabled = parse_env_var::<bool, _>("PROXY_HTTPS_ENABLED", || false)?;
        
        if https_enabled {
            // HTTPS 포트
            let https_port_raw = parse_env_var::<u16, _>("PROXY_HTTPS_PORT", || 443)?;
            let https_port = ValidPort::new(https_port_raw).ok_or_else(|| 
                SettingsError::ValidationError {
                    field: "https_port".to_string(),
                    message: format!("HTTPS 포트는 0이 될 수 없습니다: {}", https_port_raw)
                }
            )?;
            
            // TLS 인증서 및 키 경로
            let tls_cert_path = env::var("PROXY_TLS_CERT").map_err(|_| 
                SettingsError::ValidationError {
                    field: "tls_cert_path".to_string(),
                    message: "HTTPS가 활성화된 경우 TLS 인증서 경로가 필요합니다".to_string()
                }
            )?;
            
            let tls_key_path = env::var("PROXY_TLS_KEY").map_err(|_| 
                SettingsError::ValidationError {
                    field: "tls_key_path".to_string(),
                    message: "HTTPS가 활성화된 경우 TLS 키 경로가 필요합니다".to_string()
                }
            )?;
            
            builder = builder
                .with_https_port(https_port)
                .with_tls_cert_path(tls_cert_path)
                .with_tls_key_path(tls_key_path);
        }
        
        Ok(builder)
    }
}

/// 두 가지 타입 중 하나를 담을 수 있는 컨테이너
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl ServerSettings {
    /// 환경 변수에서 서버 설정을 로드하고 HTTPS 활성화 여부에 따라 적절한 타입 반환
    pub fn from_env_unified() -> Result<
        Either<ServerSettings<Validated, HttpsDisabled>, ServerSettings<Validated, HttpsEnabled>>,
        SettingsError
    > {
        let builder = ServerBuilder::from_env()?;
        let https_enabled = parse_env_var::<bool, _>("PROXY_HTTPS_ENABLED", || false)?;
        
        if https_enabled {
            // HTTPS 활성화
            let raw = builder.build_https()?;
            Ok(Either::Right(raw.validated()?))
        } else {
            // HTTPS 비활성화
            let raw = builder.build_http();
            Ok(Either::Left(raw.validated()?))
        }
    }
}

// Raw + HttpsDisabled 상태에 특화된 메서드들
impl ServerSettings<Raw, HttpsDisabled> {
    /// 기본값으로 새 ServerSettings 생성
    pub fn new() -> Self {
        ServerBuilder::default().build_http()
    }
    
    /// HTTP 포트 설정
    pub fn with_http_port(mut self, port: ValidPort) -> Self {
        self.http_port = port;
        self
    }
    
    /// HTTPS 활성화 - 상태 전환
    pub fn enable_https(self, https_port: ValidPort, cert_path: String, key_path: String) 
        -> Result<ServerSettings<Raw, HttpsEnabled>, SettingsError> 
    {
        ServerBuilder::new(self.http_port)
            .with_https_port(https_port)
            .with_tls_cert_path(cert_path)
            .with_tls_key_path(key_path)
            .build_https()
    }
    
    /// 유효성 검사 - HttpsDisabled 상태에서는 간단함
    pub fn validated(self) -> Result<ServerSettings<Validated, HttpsDisabled>, SettingsError> {
        // 포트는 이미 ValidPort를 통해 검증됨
        Ok(ServerSettings {
            http_port: self.http_port,
            https_port: self.https_port,
            tls_cert_path: None,
            tls_key_path: None,
            _marker: PhantomData,
        })
    }
    
    /// 환경 변수에서 서버 설정을 로드
    pub fn from_env() -> Result<ServerSettings<Validated, HttpsDisabled>, SettingsError> {
        let builder = ServerBuilder::from_env()?;
        let https_enabled = parse_env_var::<bool, _>("PROXY_HTTPS_ENABLED", || false)?;
        
        if !https_enabled {
            let raw = builder.build_http();
            raw.validated()
        } else {
            Err(SettingsError::ValidationError {
                field: "https_enabled".to_string(),
                message: "HTTPS가 활성화되어 있어 HttpsDisabled 타입으로 로드할 수 없습니다".to_string()
            })
        }
    }
}

// Raw + HttpsEnabled 상태에 특화된 메서드들
impl ServerSettings<Raw, HttpsEnabled> {
    /// 유효성 검사 - HttpsEnabled 상태에서는 더 복잡함
    pub fn validated(self) -> Result<ServerSettings<Validated, HttpsEnabled>, SettingsError> {
        // 포트 충돌 검사
        if self.http_port.value() == self.https_port.value() {
            return Err(SettingsError::ValidationError {
                field: "https_port".to_string(),
                message: "HTTP와 HTTPS 포트는 달라야 합니다".to_string()
            });
        }
        
        // TLS 설정 검사 (상태로 보장되지만 추가 검사)
        if self.tls_cert_path.is_none() {
            return Err(SettingsError::ValidationError { 
                field: "tls_cert_path".to_string(),
                message: "HTTPS가 활성화된 경우 TLS 인증서 경로가 필요합니다".to_string()
            });
        }
        
        if self.tls_key_path.is_none() {
            return Err(SettingsError::ValidationError { 
                field: "tls_key_path".to_string(),
                message: "HTTPS가 활성화된 경우 TLS 키 경로가 필요합니다".to_string()
            });
        }
        
        Ok(ServerSettings {
            http_port: self.http_port,
            https_port: self.https_port,
            tls_cert_path: self.tls_cert_path,
            tls_key_path: self.tls_key_path,
            _marker: PhantomData,
        })
    }
    
    /// 환경 변수에서 서버 설정을 로드
    pub fn from_env() -> Result<ServerSettings<Validated, HttpsEnabled>, SettingsError> {
        let builder = ServerBuilder::from_env()?;
        let https_enabled = parse_env_var::<bool, _>("PROXY_HTTPS_ENABLED", || false)?;
        
        if https_enabled {
            let raw = builder.build_https()?;
            raw.validated()
        } else {
            Err(SettingsError::ValidationError {
                field: "https_enabled".to_string(),
                message: "HTTPS가 비활성화되어 있어 HttpsEnabled 타입으로 로드할 수 없습니다".to_string()
            })
        }
    }
}

// 직렬화/역직렬화 구현
#[derive(Serialize, Deserialize)]
pub struct TempServerSettings {
    #[serde(default = "default_http_port")]
    pub http_port: ValidPort,
    
    #[serde(default)]
    pub https_enabled: bool,
    
    #[serde(default = "default_https_port")]
    pub https_port: ValidPort,
    
    pub tls_cert_path: Option<String>,
    
    pub tls_key_path: Option<String>,
}

// TOML 파일에서 [server] 섹션을 파싱하기 위한 래퍼 구조체
#[derive(Deserialize)]
pub struct ServerWrapper {
    pub server: TempServerSettings
}

// Raw 상태 직렬화/역직렬화
impl<'de> Deserialize<'de> for ServerSettings<Raw, HttpsDisabled> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let temp = TempServerSettings::deserialize(deserializer)?;
        
        if !temp.https_enabled {
            Ok(ServerSettings {
                http_port: temp.http_port,
                https_port: temp.https_port,
                tls_cert_path: None,
                tls_key_path: None,
                _marker: PhantomData,
            })
        } else {
            // HTTPS가 활성화된 경우 다른 상태로 역직렬화해야 함
            Err(serde::de::Error::custom(
                "HTTPS가 활성화된 경우 HttpsEnabled 상태로 역직렬화해야 합니다"
            ))
        }
    }
}

impl<'de> Deserialize<'de> for ServerSettings<Raw, HttpsEnabled> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let temp = TempServerSettings::deserialize(deserializer)?;
        
        if temp.https_enabled {
            if temp.tls_cert_path.is_none() || temp.tls_key_path.is_none() {
                return Err(serde::de::Error::custom(
                    "HTTPS가 활성화된 경우 TLS 인증서와 키 경로가 필요합니다"
                ));
            }
        
        Ok(ServerSettings {
                http_port: temp.http_port,
                https_port: temp.https_port,
                tls_cert_path: temp.tls_cert_path,
                tls_key_path: temp.tls_key_path,
                _marker: PhantomData,
            })
        } else {
            // HTTPS가 비활성화된 경우 다른 상태로 역직렬화해야 함
            Err(serde::de::Error::custom(
                "HTTPS가 비활성화된 경우 HttpsDisabled 상태로 역직렬화해야 합니다"
            ))
        }
    }
}

// Validated 상태 직렬화/역직렬화
impl<'de> Deserialize<'de> for ServerSettings<Validated, HttpsDisabled> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ServerSettings::<Raw, HttpsDisabled>::deserialize(deserializer)?;
        raw.validated().map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for ServerSettings<Validated, HttpsEnabled> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ServerSettings::<Raw, HttpsEnabled>::deserialize(deserializer)?;
        raw.validated().map_err(serde::de::Error::custom)
    }
}

// 통합 역직렬화 함수 (https_enabled 값에 따라 적절한 타입으로 역직렬화)
pub fn deserialize_server_settings<'de, D>(deserializer: D) 
    -> Result<ServerSettings<Validated, HttpsDisabled>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let temp = TempServerSettings::deserialize(deserializer)?;
    
    if temp.https_enabled {
        // HTTPS 활성화 상태로 역직렬화
        let raw = ServerSettings::<Raw, HttpsEnabled> {
            http_port: temp.http_port,
            https_port: temp.https_port,
            tls_cert_path: temp.tls_cert_path,
            tls_key_path: temp.tls_key_path,
            _marker: PhantomData,
        };
        
        let validated = raw.validated().map_err(serde::de::Error::custom)?;
        
        // 지금은 HttpsDisabled 타입만 반환하기 위해 임시 변환
        // 아직 Settings에 HttpsState 타입 변수가 없으므로 이렇게 처리
        Ok(ServerSettings {
            http_port: validated.http_port,
            https_port: validated.https_port,
            tls_cert_path: None,
            tls_key_path: None,
            _marker: PhantomData,
        })
    } else {
        // HTTPS 비활성화 상태로 역직렬화
        let raw = ServerSettings::<Raw, HttpsDisabled> {
            http_port: temp.http_port,
            https_port: temp.https_port,
            tls_cert_path: None,
            tls_key_path: None,
            _marker: PhantomData,
        };
        
        raw.validated().map_err(serde::de::Error::custom)
    }
}

// 기본값 구현 (Raw, HttpsDisabled)
impl Default for ServerSettings<Raw, HttpsDisabled> {
    fn default() -> Self {
        Self::new()
    }
}

// 특수화된 기본값 구현 (Validated, HttpsDisabled)
impl Default for ServerSettings<Validated, HttpsDisabled> {
    fn default() -> Self {
        ServerSettings::<Raw, HttpsDisabled>::new()
            .validated()
            .unwrap_or_else(|_| panic!("기본 ServerSettings 검증 실패"))
    }
}

// HTTPS 활성화 상태에 대한 기본값 구현
impl Default for ServerSettings<Validated, HttpsEnabled> {
    fn default() -> Self {
        let raw = ServerBuilder::default()
            .with_https_port(default_https_port())
            .with_tls_cert_path("cert.pem".to_string())
            .with_tls_key_path("key.pem".to_string())
            .build_https()
            .expect("기본 HTTPS ServerSettings 생성 실패");
        
        raw.validated()
            .unwrap_or_else(|_| panic!("기본 HTTPS ServerSettings 검증 실패"))
    }
} 