//! 호스트 기반 라우팅을 위한 핵심 기능을 제공하는 모듈입니다.

mod error;
mod host;
mod backend;
mod matcher;
mod table;

pub use error::{RoutingError, BackendError};
pub use host::HostInfo;
pub use backend::BackendService;
pub use matcher::{PathMatcher, PathMatcherKind};
pub use table::RoutingTable; 