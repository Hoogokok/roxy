use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::future::Future;
use std::path::PathBuf;

use crate::settings::error::SettingsError;
use crate::settings::error::SettingsValidator;
use crate::settings::server::types::{HttpsEnabled, HttpsDisabled};
use crate::settings::server::utils::{ServerWrapper};
use crate::settings::tls::TlsSettings;
use crate::settings::types::ValidPort;
use crate::settings::typestate::{Raw, Validated, TypeState, ValidationErrorCollector, ContextValidatable, AsyncContextValidatable};

/// 서버 설정 정보를 담는 구조체
#[derive(Debug, Clone)]
pub struct ServerSettings<State: TypeState = Validated, HttpsState = HttpsDisabled> {
    /// HTTP 포트
    pub(crate) http_port: ValidPort,

    /// HTTPS 포트 (HttpsEnabled 상태인 경우에만 의미 있음)
    pub(crate) https_port: ValidPort,

    /// TLS 인증서 경로 (HttpsEnabled 상태인 경우에만 의미 있음)
    pub(crate) tls_cert_path: Option<String>,

    /// TLS 키 경로 (HttpsEnabled 상태인 경우에만 의미 있음)
    pub(crate) tls_key_path: Option<String>,
    
    /// 상태를 추적하기 위한 phantom data
    pub(crate) _marker: PhantomData<(State, HttpsState)>,
}

/// 기본 HTTP 포트 반환
pub fn default_http_port() -> ValidPort { 
    ValidPort::new(80).unwrap()
}

/// 기본 HTTPS 포트 반환
pub fn default_https_port() -> ValidPort { 
    ValidPort::new(443).unwrap()
}

impl<State: TypeState, HttpsState> ServerSettings<State, HttpsState> {
    /// 모든 필드를 지정하여 ServerSettings 인스턴스 생성
    pub fn create(
        http_port: ValidPort,
        https_port: ValidPort,
        tls_cert_path: Option<String>,
        tls_key_path: Option<String>,
    ) -> Self {
        Self {
            http_port,
            https_port,
            tls_cert_path,
            tls_key_path,
            _marker: PhantomData,
        }
    }

    /// HTTP 포트 조회
    pub fn http_port(&self) -> u16 {
        self.http_port.value()
    }
    
    /// TLS 인증서 경로 조회
    pub fn tls_cert_path(&self) -> Option<&String> {
        self.tls_cert_path.as_ref()
    }
    
    /// TLS 키 경로 조회
    pub fn tls_key_path(&self) -> Option<&String> {
        self.tls_key_path.as_ref()
    }
    
    /// 테스트용 서버 설정 생성
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

impl<State: TypeState> ServerSettings<State, HttpsDisabled> {
    /// HTTPS 활성화 여부 조회 (HttpsDisabled일 경우 항상 false)
    pub fn https_enabled(&self) -> bool {
        false
    }
    
    /// HTTPS 포트 조회
    pub fn https_port(&self) -> u16 {
        self.https_port.value()
    }
}

impl<State: TypeState> ServerSettings<State, HttpsEnabled> {
    /// HTTPS 활성화 여부 조회 (HttpsEnabled일 경우 항상 true)
    pub fn https_enabled(&self) -> bool {
        true
    }
    
    /// HTTPS 포트 조회
    pub fn https_port(&self) -> u16 {
        self.https_port.value()
    }
}

impl ServerSettings<Raw, HttpsDisabled> {
    /// 기본 설정으로 HTTP 서버 설정 생성
    pub fn new() -> Self {
        Self::default()
    }
    
    /// HTTP 포트 설정
    pub fn with_http_port(mut self, port: ValidPort) -> Self {
        self.http_port = port;
        self
    }
    
    /// HTTPS 활성화
    pub fn enable_https(self, https_port: ValidPort, cert_path: String, key_path: String) 
        -> Result<ServerSettings<Raw, HttpsEnabled>, SettingsError> 
    {
        Ok(ServerSettings {
            http_port: self.http_port,
            https_port,
            tls_cert_path: Some(cert_path),
            tls_key_path: Some(key_path),
            _marker: PhantomData,
        })
    }
    
    /// 설정 검증 (TypeState를 Raw에서 Validated로 변환)
    pub fn validated(self) -> Result<ServerSettings<Validated, HttpsDisabled>, SettingsError> {
        Ok(ServerSettings {
            http_port: self.http_port,
            https_port: self.https_port,
            tls_cert_path: self.tls_cert_path,
            tls_key_path: self.tls_key_path,
            _marker: PhantomData,
        })
    }

    /// 환경 변수에서 설정 로드하여 바로 검증된 설정 반환 (HTTP 전용)
    pub fn from_env() -> Result<ServerSettings<Validated, HttpsDisabled>, SettingsError> {
        use crate::settings::server::builder::{ServerBuilder, parse_env_var};
        
        // HTTPS 활성화 여부 확인
        let https_enabled = parse_env_var("HTTPS_ENABLED", || false)?;
        if https_enabled {
            return Err(SettingsError::InvalidConfig(
                "HTTPS가 활성화되어 있지만 HTTP 전용 설정을 요청했습니다.".to_string()
            ));
        }
        
        let builder = ServerBuilder::from_env()?;
        let raw_settings = builder.build_http();
        raw_settings.validated()
    }
}

impl ServerSettings<Raw, HttpsEnabled> {
    /// 설정 검증 (TypeState를 Raw에서 Validated로 변환)
    pub fn validated(self) -> Result<ServerSettings<Validated, HttpsEnabled>, SettingsError> {
        // TLS 설정 검증
        if self.tls_cert_path.is_none() {
            return Err(SettingsError::MissingField {
                field: "tls_cert_path".into(),
                context: "HTTPS가 활성화된 경우 TLS 인증서 경로는 필수입니다.".into(),
            });
        }
        
        if self.tls_key_path.is_none() {
            return Err(SettingsError::MissingField {
                field: "tls_key_path".into(),
                context: "HTTPS가 활성화된 경우 TLS 키 경로는 필수입니다.".into(),
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

    /// 환경 변수에서 설정 로드하여 바로 검증된 설정 반환 (HTTPS 지원)
    pub async fn from_env() -> Result<ServerSettings<Validated, HttpsEnabled>, SettingsError> {
        use crate::settings::server::builder::{ServerBuilder, parse_env_var};
        
        // HTTPS 활성화 여부 확인
        let https_enabled = parse_env_var("HTTPS_ENABLED", || false)?;
        if !https_enabled {
            return Err(SettingsError::InvalidConfig(
                "HTTPS가 비활성화되어 있지만 HTTPS 지원 설정을 요청했습니다.".to_string()
            ));
        }
        
        let builder = ServerBuilder::from_env()?;
        let raw_settings = builder.build_https()?;
        
        // TLS 설정 로드 (TLS 설정이 필요하기 때문에 컨텍스트 검증 필요)
        use crate::settings::tls::TlsSettings;
        
        let tls_settings = TlsSettings::<Raw>::from_env()?;
        let tls_settings = tls_settings.validated().await?;
        
        // TLS 컨텍스트와 함께 서버 설정 검증
        raw_settings.validate_with_context(&tls_settings)
    }
}

impl ContextValidatable<ServerSettings<Validated, HttpsEnabled>, TlsSettings<Validated>> for ServerSettings<Raw, HttpsEnabled> {
    type Error = SettingsError;
    
    fn validate_with_context(self, tls_context: &TlsSettings<Validated>) -> Result<ServerSettings<Validated, HttpsEnabled>, Self::Error> {
        // TLS 컨텍스트 기반 검증
        let mut validator = SettingsValidator::new();
        
        // 인증서 및 키 경로 일치 확인
        if let Some(ref cert_path) = self.tls_cert_path {
            if let Some(tls_cert) = tls_context.cert_path() {
                if cert_path != &tls_cert.to_string_lossy().to_string() {
                    validator.add_field_error(
                        "tls_cert_path",
                        format!("서버 설정의 인증서 경로({})가 TLS 설정의 인증서 경로({})와 일치하지 않습니다.",
                            cert_path, tls_cert.display())
                    );
                }
            }
        }
        
        if let Some(ref key_path) = self.tls_key_path {
            if let Some(tls_key) = tls_context.key_path() {
                if key_path != &tls_key.to_string_lossy().to_string() {
                    validator.add_field_error(
                        "tls_key_path",
                        format!("서버 설정의 키 경로({})가 TLS 설정의 키 경로({})와 일치하지 않습니다.",
                            key_path, tls_key.display())
                    );
                }
            }
        }
        
        // 포트 검증
        if self.https_port.value() != tls_context.port() {
            validator.add_field_error(
                "https_port",
                format!("서버 설정의 HTTPS 포트({})가 TLS 설정의 포트({})와 일치하지 않습니다.",
                    self.https_port.value(), tls_context.port())
            );
        }
        
        // 오류가 있으면 반환
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
            self.validate_with_context(tls_context)
        }
    }
}

impl Default for ServerSettings<Raw, HttpsDisabled> {
    fn default() -> Self {
        Self {
            http_port: default_http_port(),
            https_port: default_https_port(),
            tls_cert_path: None,
            tls_key_path: None,
            _marker: PhantomData,
        }
    }
}

impl Default for ServerSettings<Validated, HttpsDisabled> {
    fn default() -> Self {
        Self {
            http_port: default_http_port(),
            https_port: default_https_port(),
            tls_cert_path: None,
            tls_key_path: None,
            _marker: PhantomData,
        }
    }
}

impl Default for ServerSettings<Validated, HttpsEnabled> {
    fn default() -> Self {
        Self {
            http_port: default_http_port(),
            https_port: default_https_port(),
            tls_cert_path: Some("cert.pem".to_string()),
            tls_key_path: Some("key.pem".to_string()),
            _marker: PhantomData,
        }
    }
}

impl<'de> Deserialize<'de> for ServerSettings<Raw, HttpsDisabled> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        
        // 임시 구조체를 통해 역직렬화
        let wrapper = ServerWrapper::deserialize(deserializer)?;
        let temp = wrapper.server;
        
        if temp.https_enabled {
            // HTTPS가 활성화된 설정은 HttpsDisabled 타입으로 변환할 수 없음
            return Err(D::Error::custom("HTTPS 활성화 설정은 HttpsDisabled 타입으로 변환할 수 없습니다."));
        }
        
        Ok(Self::create(
            temp.http_port,
            temp.https_port,
            temp.tls_cert_path,
            temp.tls_key_path,
        ))
    }
}

impl<'de> Deserialize<'de> for ServerSettings<Raw, HttpsEnabled> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        
        // 임시 구조체를 통해 역직렬화
        let wrapper = ServerWrapper::deserialize(deserializer)?;
        let temp = wrapper.server;
        
        if !temp.https_enabled {
            // HTTPS가 비활성화된 설정은 HttpsEnabled 타입으로 변환할 수 없음
            return Err(D::Error::custom("HTTPS 비활성화 설정은 HttpsEnabled 타입으로 변환할 수 없습니다."));
        }
        
        // 필수 필드 검증
        if temp.tls_cert_path.is_none() {
            return Err(D::Error::custom("HTTPS 활성화 시 인증서 경로는 필수입니다."));
        }
        
        if temp.tls_key_path.is_none() {
            return Err(D::Error::custom("HTTPS 활성화 시 키 경로는 필수입니다."));
        }
        
        Ok(Self::create(
            temp.http_port,
            temp.https_port,
            temp.tls_cert_path,
            temp.tls_key_path,
        ))
    }
}

impl<'de> Deserialize<'de> for ServerSettings<Validated, HttpsDisabled> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        
        // Raw 상태로 먼저 역직렬화
        let raw = ServerSettings::<Raw, HttpsDisabled>::deserialize(deserializer)?;
        
        // 검증 실행
        match raw.validated() {
            Ok(validated) => Ok(validated),
            Err(e) => Err(D::Error::custom(format!("서버 설정 검증 실패: {}", e))),
        }
    }
}

impl<'de> Deserialize<'de> for ServerSettings<Validated, HttpsEnabled> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        
        // Raw 상태로 먼저 역직렬화
        let raw = ServerSettings::<Raw, HttpsEnabled>::deserialize(deserializer)?;
        
        // 검증 실행
        match raw.validated() {
            Ok(validated) => Ok(validated),
            Err(e) => Err(D::Error::custom(format!("서버 설정 검증 실패: {}", e))),
        }
    }
}

// from_env_unified 유틸리티 함수 추가 (서버 유틸리티 모듈에서 이 메서드를 호출)
impl ServerSettings {
    pub async fn from_env_unified() -> Result<
        crate::settings::server::utils::Either<ServerSettings<Validated, HttpsDisabled>, ServerSettings<Validated, HttpsEnabled>>,
        SettingsError
    > {
        use crate::settings::server::builder::parse_env_var;
        use crate::settings::server::utils::Either;
        
        // HTTPS 활성화 여부 확인
        let https_enabled = parse_env_var("HTTPS_ENABLED", || false)?;
        
        if https_enabled {
            match ServerSettings::<Raw, HttpsEnabled>::from_env().await {
                Ok(settings) => Ok(Either::Right(settings)),
                Err(e) => Err(e),
            }
        } else {
            match ServerSettings::<Raw, HttpsDisabled>::from_env() {
                Ok(settings) => Ok(Either::Left(settings)),
                Err(e) => Err(e),
            }
        }
    }
}

// 테스트 코드 추가
#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::tls::TlsSettings;

    /// 테스트용 TLS 설정 생성
    fn create_test_tls(enabled: bool, cert_path: Option<&str>, key_path: Option<&str>) -> TlsSettings<Validated> {
        TlsSettings::create_for_test(
            enabled,
            443,
            cert_path.map(PathBuf::from),
            key_path.map(PathBuf::from)
        )
    }

    /// 테스트용 서버 설정 생성
    fn create_test_server(http_port: u16, https_port: u16, cert_path: Option<&str>, key_path: Option<&str>) -> ServerSettings<Raw, HttpsEnabled> {
        ServerSettings::create(
            ValidPort::new(http_port).unwrap(),
            ValidPort::new(https_port).unwrap(),
            cert_path.map(String::from),
            key_path.map(String::from),
        )
    }

    #[tokio::test]
    async fn test_context_validation() {
        // 유효한 TLS 설정
        let tls = create_test_tls(true, Some("/path/to/cert.pem"), Some("/path/to/key.pem"));
        
        // 서버 설정 생성 (TLS 설정과 일치)
        let server = create_test_server(80, 443, Some("/path/to/cert.pem"), Some("/path/to/key.pem"));
        
        // 컨텍스트 검증 - 성공해야 함
        let result = server.validate_with_context(&tls);
        assert!(result.is_ok());
        
        // 서버 설정 생성 (포트 불일치)
        let server = create_test_server(80, 8443, Some("/path/to/cert.pem"), Some("/path/to/key.pem"));
        
        // 컨텍스트 검증 - 실패해야 함 (포트 불일치)
        let result = server.validate_with_context(&tls);
        assert!(result.is_err());
        
        // 서버 설정 생성 (인증서 경로 불일치)
        let server = create_test_server(80, 443, Some("/path/to/other-cert.pem"), Some("/path/to/key.pem"));
        
        // 컨텍스트 검증 - 실패해야 함 (인증서 경로 불일치)
        let result = server.validate_with_context(&tls);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_async_context_validation() {
        // 유효한 TLS 설정
        let tls = create_test_tls(true, Some("/path/to/cert.pem"), Some("/path/to/key.pem"));
        
        // 서버 설정 생성 (TLS 설정과 일치)
        let server = create_test_server(80, 443, Some("/path/to/cert.pem"), Some("/path/to/key.pem"));
        
        // 비동기 컨텍스트 검증 - 성공해야 함
        let result = server.validate_with_context_async(&tls).await;
        assert!(result.is_ok());
        
        // 서버 설정 생성 (포트 불일치)
        let server = create_test_server(80, 8443, Some("/path/to/cert.pem"), Some("/path/to/key.pem"));
        
        // 비동기 컨텍스트 검증 - 실패해야 함 (포트 불일치)
        let result = server.validate_with_context_async(&tls).await;
        assert!(result.is_err());
    }
} 