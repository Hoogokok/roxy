use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::Deserialize;

use crate::middleware::config::MiddlewareConfig;
use crate::settings::types::ValidMiddlewareId;
use crate::settings::server::{self, ServerSettings, Raw, HttpsDisabled, HttpsEnabled};
use crate::settings::tls::TlsSettings;
use crate::settings::logging::LogSettings;
use crate::settings::docker::DockerSettings;
use crate::settings::{SettingsError, Settings, Either, Result, parse_env_var};

/// 검증되지 않은 원시 설정을 나타내는 구조체
#[derive(Deserialize)]
pub struct RawSettings<HttpsState = HttpsDisabled> {
    #[serde(skip_deserializing)]
    pub server: ServerSettings<Raw, HttpsState>,
    
    #[serde(default)]
    pub logging: LogSettings,
    
    #[serde(default)]
    pub tls: TlsSettings<Raw>,
    
    #[serde(default)]
    pub docker: DockerSettings,
    
    #[serde(default)]
    pub middleware: HashMap<String, MiddlewareConfig>,
    
    #[serde(default)]
    pub router_middlewares: HashMap<String, Vec<ValidMiddlewareId>>,
}

impl<HttpsState> RawSettings<HttpsState> {
    /// 기본 RawSettings 인스턴스 생성
    pub fn new(
        server: ServerSettings<Raw, HttpsState>,
        logging: LogSettings,
        tls: TlsSettings<Raw>,
        docker: DockerSettings,
        middleware: HashMap<String, MiddlewareConfig>,
        router_middlewares: HashMap<String, Vec<ValidMiddlewareId>>,
    ) -> Self {
        Self {
            server,
            logging,
            tls,
            docker,
            middleware,
            router_middlewares,
        }
    }
}

impl RawSettings<HttpsDisabled> {
    /// 환경 변수에서 설정 로드
    pub fn from_env() -> Result<Self> {
        // 서버 빌더 생성
        let server_builder = server::ServerBuilder::from_env()?;
        // HTTP 전용 서버 생성 (Raw 상태)
        let server = server_builder.build_http();
        
        let logging = LogSettings::from_env()?;
        let tls = TlsSettings::<Raw>::from_env()?;
        let docker = DockerSettings::from_env()?;
        
        Ok(Self {
            server,
            logging,
            tls,
            docker,
            middleware: HashMap::new(),
            router_middlewares: HashMap::new(),
        })
    }
    
    /// Raw 상태에서 Validated 상태로 변환
    pub async fn validate(self) -> Result<Settings<HttpsDisabled>> {
        // 각 컴포넌트 검증
        let validated_server = self.server.validated()?;
        let validated_tls = self.tls.validated().await?;
        
        // 검증된 설정으로 Settings 생성
        let settings = Settings {
            server: validated_server,
            logging: self.logging,
            tls: validated_tls,
            docker: self.docker,
            middleware: self.middleware,
            router_middlewares: self.router_middlewares,
        };
        
        // 추가 검증이 필요한 경우
        settings.validate().await?;
        
        Ok(settings)
    }
}

impl RawSettings<HttpsEnabled> {
    /// 환경 변수에서 HTTPS 활성화된 설정 로드
    pub fn from_env() -> Result<Self> {
        // 서버 빌더 생성
        let server_builder = server::ServerBuilder::from_env()?;
        // HTTPS 설정 확인
        let https_port = parse_env_var::<u16, _>("PROXY_HTTPS_PORT", || 443)?;
        let tls_cert = std::env::var("PROXY_TLS_CERT").ok();
        let tls_key = std::env::var("PROXY_TLS_KEY").ok();
        
        // 서버 빌더 설정
        let server_builder = server_builder.with_https_port(server::ValidPort::new(https_port).unwrap());
        
        // TLS 인증서/키 설정
        let server_builder = match (tls_cert, tls_key) {
            (Some(cert), Some(key)) => server_builder.with_tls_cert_path(cert).with_tls_key_path(key),
            _ => return Err(SettingsError::ValidationError {
                field: "tls_paths".to_string(),
                message: "HTTPS가 활성화된 경우 TLS 인증서와 키 경로가 필요합니다".to_string()
            }),
        };
        
        // HTTPS 서버 생성 (Raw 상태)
        let server = server_builder.build_https()?;
        
        let logging = LogSettings::from_env()?;
        let tls = TlsSettings::<Raw>::from_env()?;
        let docker = DockerSettings::from_env()?;
        
        Ok(Self {
            server,
            logging,
            tls,
            docker,
            middleware: HashMap::new(),
            router_middlewares: HashMap::new(),
        })
    }
    
    /// Raw 상태에서 Validated 상태로 변환
    pub async fn validate(self) -> Result<Settings<HttpsEnabled>> {
        // 각 컴포넌트 검증
        let validated_server = self.server.validated()?;
        let validated_tls = self.tls.validated().await?;
        
        // 검증된 설정으로 Settings 생성
        let settings = Settings {
            server: validated_server,
            logging: self.logging,
            tls: validated_tls,
            docker: self.docker,
            middleware: self.middleware,
            router_middlewares: self.router_middlewares,
        };
        
        // 추가 검증이 필요한 경우
        settings.validate().await?;
        
        Ok(settings)
    }
    
    /// TOML 파일에서 설정 로드 및 검증 헬퍼 함수
    pub async fn from_toml_file<P: AsRef<Path>>(path: P) -> Result<Either<Settings<HttpsDisabled>, Settings<HttpsEnabled>>> {
        // 파일 내용 읽기
        let content = std::fs::read_to_string(path.as_ref())?;
        
        // TOML 역직렬화를 위한 헬퍼 구조체
        #[derive(Deserialize)]
        struct SettingsHelper {
            #[serde(default)]
            logging: LogSettings,
            #[serde(default)]
            tls: TlsSettings<Raw>,
            #[serde(default)]
            docker: DockerSettings,
            #[serde(default)]
            middleware: HashMap<String, MiddlewareConfig>,
            #[serde(default)]
            router_middlewares: HashMap<String, Vec<ValidMiddlewareId>>,
            #[serde(default)]
            https_enabled: bool,
        }
        
        // 역직렬화
        let helper: SettingsHelper = toml::from_str(&content)?;
        
        // TLS 설정 검증
        let validated_tls = helper.tls.validated().await?;
        
        // HTTPS 활성화 여부에 따라 적절한 설정 생성
        if helper.https_enabled || validated_tls.is_enabled() {
            // HTTPS 서버 설정 생성
            let server_builder = server::ServerBuilder::new(server::default_http_port())
                .with_https_port(server::default_https_port());
            let server = server_builder.build_https()?;
            let validated_server = server.validated()?;
            
            let settings = Settings {
                server: validated_server,
                logging: helper.logging,
                tls: validated_tls,
                docker: helper.docker,
                middleware: helper.middleware,
                router_middlewares: helper.router_middlewares,
            };
            
            settings.validate().await?;
            Ok(Either::Right(settings))
        } else {
            // HTTP 서버 설정 생성
            let server_builder = server::ServerBuilder::new(server::default_http_port());
            let server = server_builder.build_http();
            let validated_server = server.validated()?;
            
            let settings = Settings {
                server: validated_server,
                logging: helper.logging,
                tls: validated_tls,
                docker: helper.docker,
                middleware: helper.middleware,
                router_middlewares: helper.router_middlewares,
            };
            
            settings.validate().await?;
            Ok(Either::Left(settings))
        }
    }
}