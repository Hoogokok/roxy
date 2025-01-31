//! Reverse Proxy Traefik은 호스트 기반 라우팅을 지원하는 경량 리버스 프록시입니다.
//! 
//! # 주요 기능
//! 
//! - 호스트 기반 라우팅
//! - 동적 백엔드 서비스 관리
//! - 에러 처리 및 로깅
//! 
//! # 예제
//! 
//! ```no_run
//! use reverse_proxy_traefik::routing::{RoutingTable, BackendService};
//! 
//! // 라우팅 테이블 초기화
//! let mut table = RoutingTable::new();
//! 
//! // 백엔드 서비스 추가
//! table.add_route(
//!     "example.com".to_string(),
//!     BackendService {
//!         address: "127.0.0.1:8080".parse().unwrap(),
//!     },
//! );
//! ```

pub mod routing; 