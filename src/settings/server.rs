use serde::{Deserialize, Serialize};
use std::env;
use std::marker::PhantomData;
use super::SettingsError;
use super::types::ValidPort;
use crate::settings::typestate::{Raw, Validated, TypeState, ValidationErrorCollector, ContextValidatable, AsyncContextValidatable};
use crate::settings::error::SettingsValidator;
use crate::settings::tls::TlsSettings;
use std::future::Future;
use std::path::PathBuf;

// HTTPS 활성화 상태를 표현하는 타입
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpsEnabled;

/// HTTPS 비활성화 상태를 나타내는 타입
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpsDisabled;

impl TypeState for HttpsDisabled {}
impl TypeState for HttpsEnabled {}

// HttpsState 타입 매개변수가 추가된 ServerSettings
#[derive(Debug, Clone)]
pub struct ServerSettings<State: TypeState = Validated, HttpsState = HttpsDisabled> {
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

pub fn default_http_port() -> ValidPort { 
    ValidPort::new(80).expect("기본 HTTP 포트는 항상 유효해야 합니다") 
}

pub fn default_https_port() -> ValidPort { 
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

// 일반 메서드 블록 - TypeState 트레이트 경계 추가
impl<State: TypeState, HttpsState> ServerSettings<State, HttpsState> {
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

    /// 테스트용 ServerSettings 생성 (테스트 코드에서만 사용)
    #[cfg(test)]
    pub fn create_for_test(
        http_port: ValidPort,
        https_port: ValidPort,
        tls_cert_path: Option<String>,
        tls_key_path: Option<String>
    ) -> Self {
        Self {
            http_port,
            https_port,
            tls_cert_path,
            tls_key_path,
            _marker: PhantomData,
        }
    }
}

// HttpsDisabled 상태 메서드
impl<State: TypeState> ServerSettings<State, HttpsDisabled> {
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
impl<State: TypeState> ServerSettings<State, HttpsEnabled> {
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

// Raw 상태 메서드 블록
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
            Err(SettingsError::MissingField {
                field: "https_enabled".to_string(),
                context: "HTTPS가 활성화되어 있어 HttpsDisabled 타입으로 로드할 수 없습니다".to_string()
            })
        }
    }
}

impl ServerSettings<Raw, HttpsEnabled> {
    /// 유효성 검사 - HttpsEnabled 상태에서는 더 복잡함
    pub fn validated(self) -> Result<ServerSettings<Validated, HttpsEnabled>, SettingsError> {
        // 참고: 컨텍스트 기반 검증을 위해 ContextValidatable.validate_with_context 사용 권장
        // 이 메서드는 하위 호환성을 위해 유지됩니다.
        let mut validator = SettingsValidator::new();
        validator.start_collecting();
        
        // 포트 충돌 검사
        if self.http_port.value() == self.https_port.value() {
            validator.add_error(SettingsError::InvalidValue {
                field: "https_port".to_string(),
                context: "서버 설정".to_string(),
                message: "HTTP와 HTTPS 포트는 달라야 합니다".to_string()
            });
        }
        
        // TLS 설정 검사 (상태로 보장되지만 추가 검사)
        if self.tls_cert_path.is_none() {
            validator.add_error(SettingsError::MissingField { 
                field: "tls_cert_path".to_string(),
                context: "서버 설정".to_string()
            });
        }
        
        if self.tls_key_path.is_none() {
            validator.add_error(SettingsError::MissingField { 
                field: "tls_key_path".to_string(),
                context: "서버 설정".to_string()
            });
        }
        
        if validator.has_errors() {
            return Err(validator.into_error());
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
        ServerBuilder::default().build_http()
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

impl ContextValidatable<ServerSettings<Validated, HttpsEnabled>, TlsSettings<Validated>> for ServerSettings<Raw, HttpsEnabled> {
    type Error = SettingsError;

    fn validate_with_context(self, tls_context: &TlsSettings<Validated>) -> Result<ServerSettings<Validated, HttpsEnabled>, Self::Error> {
        let mut validator = SettingsValidator::new();
        validator.start_collecting();
        
        // 포트 충돌 검사
        if self.http_port.value() == self.https_port.value() {
            validator.add_error(SettingsError::InvalidValue {
                field: "https_port".to_string(),
                context: "서버 설정".to_string(),
                message: "HTTP와 HTTPS 포트는 달라야 합니다".to_string()
            });
        }
        
        // TLS 컨텍스트와의 일관성 검사
        if !tls_context.is_enabled() {
            validator.add_error(SettingsError::InvalidConfig(
                "HTTPS 서버는 TLS가 활성화되어야 합니다".to_string()
            ));
        }
        
        // TLS 설정 검사 (상태로 보장되지만 추가 검사)
        if self.tls_cert_path.is_none() {
            validator.add_error(SettingsError::MissingField { 
                field: "tls_cert_path".to_string(),
                context: "서버 설정".to_string()
            });
        }
        
        if self.tls_key_path.is_none() {
            validator.add_error(SettingsError::MissingField { 
                field: "tls_key_path".to_string(),
                context: "서버 설정".to_string()
            });
        }
        
        // TLS 인증서 경로와 설정의 일관성 검사
        if let Some(cert_path) = &self.tls_cert_path {
            if let Some(tls_cert_path) = tls_context.cert_path() {
                if cert_path != &tls_cert_path.to_string_lossy().to_string() {
                    validator.add_error(SettingsError::InvalidValue {
                        field: "tls_cert_path".to_string(),
                        context: "서버 설정".to_string(),
                        message: format!("서버 TLS 인증서 경로 ({})와 TLS 설정의 인증서 경로 ({})가 일치하지 않습니다", 
                            cert_path, tls_cert_path.to_string_lossy())
                    });
                }
            }
        }
        
        if validator.has_errors() {
            return Err(validator.into_error());
        }
        
        Ok(ServerSettings {
            http_port: self.http_port,
            https_port: self.https_port,
            tls_cert_path: self.tls_cert_path,
            tls_key_path: self.tls_key_path,
            _marker: PhantomData,
        })
    }
}

impl AsyncContextValidatable<ServerSettings<Validated, HttpsEnabled>, TlsSettings<Validated>> for ServerSettings<Raw, HttpsEnabled> {
    type Error = SettingsError;

    fn validate_with_context_async(self, tls_context: &TlsSettings<Validated>) -> impl Future<Output = Result<ServerSettings<Validated, HttpsEnabled>, Self::Error>> + Send
    where
        Self: Send,
    {
        async move {
            // 컨텍스트 기반 검증 단계에서 추가적인 비동기 검사 가능
            // 예: 비동기 파일 시스템 확인, 네트워크 리소스 확인 등
            
            // 기본 구현은 동기식 validate_with_context 호출
            self.validate_with_context(tls_context)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::settings::tls::TlsSettings;

    // 테스트용 TlsSettings 생성 헬퍼 함수
    fn create_test_tls(enabled: bool, cert_path: Option<&str>, key_path: Option<&str>) -> TlsSettings<Validated> {
        // 테스트 전용 함수를 사용
        TlsSettings::create_for_test(
            enabled, 
            443, 
            cert_path.map(PathBuf::from), 
            key_path.map(PathBuf::from)
        )
    }
    
    // 테스트용 서버 설정 생성 헬퍼 함수
    fn create_test_server(http_port: u16, https_port: u16, cert_path: Option<&str>, key_path: Option<&str>) -> ServerSettings<Raw, HttpsEnabled> {
        // 테스트 전용 함수를 사용
        ServerSettings::create_for_test(
            ValidPort::new(http_port).unwrap(),
            ValidPort::new(https_port).unwrap(),
            cert_path.map(String::from),
            key_path.map(String::from)
        )
    }

    #[tokio::test]
    async fn test_context_validation() {
        // 유효한 TLS + 서버 설정 테스트
        {
            let tls_validated = create_test_tls(true, Some("cert.pem"), Some("key.pem"));
            let server_raw = create_test_server(80, 443, Some("cert.pem"), Some("key.pem"));

            // 컨텍스트 기반 검증 실행
            let result = server_raw.validate_with_context(&tls_validated);
            assert!(result.is_ok());
        }

        // TLS 비활성화 테스트
        {
            let tls_disabled = create_test_tls(false, None, None);
            let server_raw = create_test_server(80, 443, Some("cert.pem"), Some("key.pem"));

            // 컨텍스트 기반 검증 실행 - 실패해야 함
            let invalid_result = server_raw.validate_with_context(&tls_disabled);
            assert!(invalid_result.is_err());
        }
        
        // 경로 불일치 테스트
        {
            let tls_different_path = create_test_tls(true, Some("different_cert.pem"), Some("key.pem"));
            let server_raw = create_test_server(80, 443, Some("cert.pem"), Some("key.pem"));
            
            let path_mismatch_result = server_raw.validate_with_context(&tls_different_path);
            assert!(path_mismatch_result.is_err());
        }
    }
    
    #[tokio::test]
    async fn test_async_context_validation() {
        let tls_validated = create_test_tls(true, Some("cert.pem"), Some("key.pem"));
        let server_raw = create_test_server(80, 443, Some("cert.pem"), Some("key.pem"));

        // 비동기 컨텍스트 기반 검증 실행
        let result = server_raw.validate_with_context_async(&tls_validated).await;
        assert!(result.is_ok());
    }
}
