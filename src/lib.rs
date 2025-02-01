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
//! ```
//! use reverse_proxy_traefik::routing::{RoutingTable, BackendService};
//! use std::net::SocketAddr;
//! 
//! let mut table = RoutingTable::new();
//! 
//! // 백엔드 서비스 생성
//! let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
//! let backend = BackendService::new(addr);
//! 
//! // 라우팅 규칙 추가
//! table.add_route("example.com".to_string(), backend);
//! ```
//! 
//! # 로드 밸런싱
//! 
//! ```
//! use reverse_proxy_traefik::routing::{RoutingTable, BackendService};
//! use std::net::SocketAddr;
//! 
//! let mut table = RoutingTable::new();
//! 
//! // 첫 번째 백엔드 추가
//! let addr1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
//! let backend1 = BackendService::new(addr1);
//! table.add_route("example.com".to_string(), backend1);
//! 
//! // 동일 호스트에 두 번째 백엔드 추가 (자동으로 로드 밸런싱됨)
//! let addr2: SocketAddr = "127.0.0.1:8081".parse().unwrap();
//! let backend2 = BackendService::new(addr2);
//! table.add_route("example.com".to_string(), backend2);
//! ```

pub mod routing; 