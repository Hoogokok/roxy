//! 호스트 기반 라우팅을 위한 핵심 기능을 제공하는 모듈입니다.

mod error;
mod host;
mod backend;

pub use error::{RoutingError, BackendError};
pub use host::HostInfo;
pub use backend::BackendService; 