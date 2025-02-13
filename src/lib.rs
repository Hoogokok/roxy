//! Reverse Proxy Traefik은 호스트 기반 라우팅을 지원하는 경량 리버스 프록시입니다.
//! 
//! # 주요 기능
//! 
//! - 호스트 기반 라우팅
//! - 동적 백엔드 서비스 관리
//! - 에러 처리 및 로깅
//! - 설정 파일(TOML) 및 환경변수 지원
//! 
//! # 예제
//! 
//! ```
//! use reverse_proxy_traefik::{
//!     routing_v2::{RoutingTable, BackendService, PathMatcher},
//!     settings::Settings,
//! };
//! use std::net::SocketAddr;
//! 
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // 설정 로드
//! let settings = Settings::load()?;
//! 
//! let mut table = RoutingTable::new();
//! 
//! // 백엔드 서비스 생성
//! let addr: SocketAddr = "127.0.0.1:8080".parse()?;
//! let backend = BackendService::new(addr);
//! 
//! // 라우팅 규칙 추가 - API 서비스 (PathPrefix 매칭)
//! table.add_route(
//!     "example.com".to_string(), 
//!     backend,
//!     Some(PathMatcher::from_str("/api*").unwrap())
//! );
//! # Ok(())
//! # }
//! ```
//! 
//! # 설정 예시
//! 
//! ```toml
//! [server]
//! http_port = 8080
//! https_enabled = true
//! https_port = 443
//! 
//! [logging]
//! format = "json"
//! level = "info"
//! 
//! [docker]
//! network = "reverse-proxy-network"
//! label_prefix = "reverse-proxy."
//! ```
//! 
//! # 로드 밸런싱
//! 
//! ```
//! use reverse_proxy_traefik::routing_v2::{RoutingTable, BackendService};
//! use std::net::SocketAddr;
//! 
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut table = RoutingTable::new();
//! 
//! // 첫 번째 백엔드 추가
//! let addr1: SocketAddr = "127.0.0.1:8080".parse()?;
//! let backend1 = BackendService::new(addr1);
//! table.add_route("example.com".to_string(), backend1, None);
//! 
//! // 동일 호스트에 두 번째 백엔드 추가 (자동으로 로드 밸런싱됨)
//! let addr2: SocketAddr = "127.0.0.1:8081".parse()?;
//! let backend2 = BackendService::new(addr2);
//! table.add_route("example.com".to_string(), backend2, None);
//! # Ok(())
//! # }
//! ```
//! 
//! # 모듈 구조
//! 
//! - `settings`: 설정 관리
//!   - 환경변수와 TOML 파일에서 설정 로드
//!   - 서버, 로깅, TLS, Docker 설정 관리
//! - `server`: HTTP/HTTPS 서버 구현
//!   - 요청 처리 및 라우팅
//!   - TLS 지원
//! - `routing_v2`: 라우팅 로직
//!   - 호스트 및 경로 기반 라우팅
//!   - 로드 밸런싱
//! - `docker`: Docker 통합
//!   - 컨테이너 검색 및 모니터링
//!   - 동적 라우팅 설정
//! - `middleware`: 미들웨어 시스템
//!   - 기본 인증
//!   - 헤더 조작
//! 
//! # 환경변수 설정
//! 
//! ```text
//! # 서버 설정
//! PROXY_HTTP_PORT=8080
//! PROXY_HTTPS_ENABLED=true
//! PROXY_HTTPS_PORT=443
//! 
//! # 로깅 설정
//! PROXY_LOG_FORMAT=json
//! PROXY_LOG_LEVEL=info
//! PROXY_LOG_OUTPUT=stdout
//! 
//! # Docker 설정
//! PROXY_DOCKER_NETWORK=reverse-proxy-network
//! PROXY_DOCKER_LABEL_PREFIX=reverse-proxy.
//! ```
//! 
//! # 미들웨어 설정 예시
//! 
//! ```toml
//! [middleware.auth]
//! middleware-type = "basic-auth"
//! enabled = true
//! order = 1
//! 
//! [middleware.auth.settings]
//! users = { "admin" = "password" }
//! 
//! [middleware.headers]
//! middleware-type = "headers"
//! enabled = true
//! order = 2
//! 
//! [middleware.headers.settings]
//! request_headers = { "X-Real-IP" = "$remote_addr" }
//! response_headers = { "X-Powered-By" = "ReverseProxy" }
//! ```

// 주요 타입들을 최상위에서 바로 사용할 수 있도록 re-export
pub use crate::{
    settings::Settings,
    server::ServerManager,
    docker::DockerManager,
    routing_v2::{RoutingTable, BackendService, PathMatcher},
};

/// # 에러 처리
/// 
/// ```
/// use reverse_proxy_traefik::settings::SettingsError;
/// 
/// fn handle_settings_error(err: SettingsError) {
///     match err {
///         SettingsError::EnvVarMissing { var_name } => {
///             eprintln!("필수 환경변수가 없습니다: {}", var_name);
///         }
///         SettingsError::EnvVarInvalid { var_name, value, reason } => {
///             eprintln!("잘못된 환경변수: {}={} ({})", var_name, value, reason);
///         }
///     }
/// }
/// ```
pub mod logging;
pub mod proxy;
pub mod tls;
pub mod docker;
pub mod routing_v2;
pub mod middleware;
pub mod settings;
pub mod server;  // server 모듈 추가