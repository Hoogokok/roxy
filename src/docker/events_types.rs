use std::collections::HashMap;
use crate::routing::BackendService;
use crate::docker::DockerError; // DockerError가 정의된 경로에 맞게 수정하세요.

#[derive(Debug)]
pub enum DockerEvent {
    /// 컨테이너 시작
    ContainerStarted {
        container_id: String,
        host: String,
        service: BackendService,
        path: Option<String>,
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
        path: Option<String>,
    },
    /// 에러 상황
    Error(DockerError),
    /// 라우팅 테이블 업데이트
    RoutesUpdated( HashMap<(String, Option<String>), BackendService>),
}