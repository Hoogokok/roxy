//! 호스트 기반 라우팅을 위한 핵심 기능을 제공하는 모듈입니다.

pub mod backend;
pub mod error;
mod host;
pub mod matcher;
mod table;

pub use backend::{BackendService, LoadBalancer, LoadBalancerStrategy};
pub use error::{RoutingError, BackendError};
pub use host::HostInfo;
pub use matcher::PathMatcher;
pub use table::RoutingTable; 