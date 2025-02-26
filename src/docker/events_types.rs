use std::collections::HashMap;
use crate::docker::DockerError;
use crate::routing_v2::{BackendService, PathMatcher};
use crate::middleware::MiddlewareConfig;

/// 컨테이너 헬스 상태
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    /// 상태 확인 전
    Unknown,
    /// 정상
    Healthy,
    /// 비정상
    Unhealthy,
    /// 체크 중
    Starting,
}

#[derive(Debug)]
pub enum DockerEvent {
    /// 컨테이너 시작
    ContainerStarted {
        container_id: String,
        host: String,
        service: BackendService,
        path_matcher: Option<PathMatcher>,
    },
    /// 컨테이너 중지
    ContainerStopped {
        container_id: String,
        host: String,
    },
    /// 컨테이너 설정 변경
    ContainerUpdated {
        container_id: String,
        old_host: Option<String>,
        new_host: Option<String>,
        service: Option<BackendService>,
        path_matcher: Option<PathMatcher>,
    },
    /// 에러 상황
    Error(DockerError),
    /// 라우팅 테이블 업데이트
    RoutesUpdated(HashMap<(String, PathMatcher), BackendService>),
    MiddlewareConfigsUpdated(Vec<(String, MiddlewareConfig)>),
    /// 컨테이너 헬스 상태 변경
    ContainerHealthChanged {
        container_id: String,
        status: HealthStatus,
        message: String,
        host: String,
        consecutive_failures: u64,
    },
}