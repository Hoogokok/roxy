use std::env;
use serde::{Deserialize, Serialize};

use crate::settings::error::SettingsError;
use crate::settings::server::settings::{ServerSettings, default_http_port, default_https_port};
use crate::settings::server::types::{HttpsEnabled, HttpsDisabled};
use crate::settings::types::ValidPort;
use crate::settings::typestate::{Raw, Validated};

/// 좌측 또는 우측 값을 가질 수 있는 열거형 (Either 타입)
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

/// HTTP/HTTPS 서버 설정 로드를 위한 통합 기능
pub async fn from_env_unified() -> Result<
    Either<ServerSettings<Validated, HttpsDisabled>, 
           ServerSettings<Validated, HttpsEnabled>>,
    SettingsError
> {
    // ServerSettings의 from_env_unified 메서드 호출
    ServerSettings::from_env_unified().await
}

/// 역직렬화 과정에서 필요한 임시 서버 설정 구조체
#[derive(Deserialize, Serialize)]
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

/// 역직렬화 과정에서 필요한 래퍼 구조체
#[derive(Deserialize, Serialize)]
pub struct ServerWrapper {
    pub server: TempServerSettings
}

/// ServerSettings<Raw, HttpsDisabled>에 대한 역직렬화 지원
pub fn deserialize_server_settings<'de, D>(deserializer: D) 
    -> Result<ServerSettings<Validated, HttpsDisabled>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // 직접 ServerSettings<Validated, HttpsDisabled>::deserialize 호출
    ServerSettings::<Validated, HttpsDisabled>::deserialize(deserializer)
} 