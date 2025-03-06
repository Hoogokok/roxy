use serde::Deserialize;
use std::env;
use std::marker::PhantomData;
use super::SettingsError;

// 상태를 표현하는 타입
#[derive(Debug, Clone, Copy)]
pub struct Raw;

#[derive(Debug, Clone, Copy)]
pub struct Validated;

#[derive(Clone, Debug)]
pub struct ServerSettings<State = Validated> {
    /// HTTP 포트 (기본값: 8080)
    http_port: u16,

    /// HTTPS 활성화 여부
    https_enabled: bool,

    /// HTTPS 포트 (기본값: 443)
    https_port: u16,

    /// TLS 인증서 경로
    tls_cert_path: Option<String>,

    /// TLS 키 경로
    tls_key_path: Option<String>,
    
    /// 상태를 추적하기 위한 phantom data
    _marker: PhantomData<State>,
}

fn default_http_port() -> u16 { 80 }
fn default_https_port() -> u16 { 443 }

fn default_https_disabled() -> bool { false }

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

// 두 상태에 공통적인 메서드들
impl<State> ServerSettings<State> {
    /// HTTP 포트 getter
    pub fn http_port(&self) -> u16 {
        self.http_port
    }
    
    /// HTTPS 활성화 여부 getter
    pub fn https_enabled(&self) -> bool {
        self.https_enabled
    }
    
    /// HTTPS 포트 getter
    pub fn https_port(&self) -> u16 {
        self.https_port
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

// Raw 상태에 특화된 메서드들
impl ServerSettings<Raw> {
    /// 기본값으로 새 ServerSettings 생성
    pub fn new() -> Self {
        Self {
            http_port: default_http_port(),
            https_enabled: false,
            https_port: default_https_port(),
            tls_cert_path: None,
            tls_key_path: None,
            _marker: PhantomData,
        }
    }
    
    /// HTTP 포트 설정
    pub fn with_http_port(mut self, port: u16) -> Self {
        self.http_port = port;
        self
    }
    
    /// HTTPS 활성화 여부 설정
    pub fn with_https_enabled(mut self, enabled: bool) -> Self {
        self.https_enabled = enabled;
        self
    }
    
    /// HTTPS 포트 설정
    pub fn with_https_port(mut self, port: u16) -> Self {
        self.https_port = port;
        self
    }
    
    /// TLS 인증서 경로 설정
    pub fn with_tls_cert_path(mut self, path: Option<String>) -> Self {
        self.tls_cert_path = path;
        self
    }
    
    /// TLS 키 경로 설정
    pub fn with_tls_key_path(mut self, path: Option<String>) -> Self {
        self.tls_key_path = path;
        self
    }
    
    /// 포트 문자열 파싱 (헬퍼 메서드)
    fn parse_port(name: &str, value: &str) -> Result<u16, SettingsError> {
        match value.parse::<u16>() {
            Ok(port) => Ok(port),
            Err(e) => Err(SettingsError::EnvVarInvalid {
                var_name: name.to_string(),
                value: value.to_string(),
                reason: format!("포트 번호가 유효하지 않음: {}", e)
            })
        }
    }
    
    /// 환경 변수에서 서버 설정을 로드합니다.
    pub fn from_env() -> Result<ServerSettings<Validated>, SettingsError> {
        let http_port = parse_env_var("SERVER_HTTP_PORT", default_http_port)?;
        let https_enabled = parse_env_var("SERVER_HTTPS_ENABLED", || false)?;
        let https_port = parse_env_var("SERVER_HTTPS_PORT", default_https_port)?;
        
        let tls_cert_path = match env::var("SERVER_TLS_CERT_PATH") {
            Ok(path) => Some(path),
            Err(_) => None
        };
        
        let tls_key_path = match env::var("SERVER_TLS_KEY_PATH") {
            Ok(path) => Some(path),
            Err(_) => None
        };
        
        let raw = ServerSettings::<Raw> {
            http_port,
            https_enabled,
            https_port,
            tls_cert_path,
            tls_key_path,
            _marker: PhantomData,
        };
        
        raw.validated()
    }
    
    /// 유효성 검사 실행 후 Validated 상태로 전환
    pub fn validated(self) -> Result<ServerSettings<Validated>, SettingsError> {
        // 유효성 검사 로직
        if self.https_enabled {
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
        }
        
        // 검증 성공 시 Validated 상태의 인스턴스 반환
        Ok(ServerSettings {
            http_port: self.http_port,
            https_enabled: self.https_enabled,
            https_port: self.https_port,
            tls_cert_path: self.tls_cert_path,
            tls_key_path: self.tls_key_path,
            _marker: PhantomData,
        })
    }
}

// 기존 코드와의 호환성을 위한 Deserialize 구현
impl<'de> Deserialize<'de> for ServerSettings<Raw> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // 임시 구조체를 통해 역직렬화
        #[derive(Deserialize)]
        struct TempServerSettings {
            #[serde(default = "default_http_port")]
            http_port: u16,
            
            #[serde(default)]
            https_enabled: bool,
            
            #[serde(default = "default_https_port")]
            https_port: u16,
            
            tls_cert_path: Option<String>,
            
            tls_key_path: Option<String>,
        }
        
        let temp = TempServerSettings::deserialize(deserializer)?;
        
        Ok(ServerSettings {
            http_port: temp.http_port,
            https_enabled: temp.https_enabled,
            https_port: temp.https_port,
            tls_cert_path: temp.tls_cert_path,
            tls_key_path: temp.tls_key_path,
            _marker: PhantomData,
        })
    }
}

// 기존 코드와의 호환성을 위한 Default 구현
impl Default for ServerSettings<Raw> {
    fn default() -> Self {
        Self::new()
    }
}

// Validated 상태에 대한 Default 구현
impl Default for ServerSettings<Validated> {
    fn default() -> Self {
        ServerSettings::<Raw>::new().validated().unwrap_or_else(|_| panic!("기본 ServerSettings 검증 실패"))
    }
}

impl<'de> Deserialize<'de> for ServerSettings<Validated> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ServerSettings::<Raw>::deserialize(deserializer)?;
        raw.validated().map_err(serde::de::Error::custom)
    }
} 