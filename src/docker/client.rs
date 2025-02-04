// src/docker/client.rs
use async_trait::async_trait;
use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage};
use bollard::system::EventsOptions;
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use crate::docker::DockerError;

#[async_trait]
pub trait DockerClient: Send + Sync {
    async fn list_containers(
        &self, 
        options: Option<ListContainersOptions<String>>
    ) -> Result<Vec<ContainerSummary>, DockerError>;

    fn events(
        &self, 
        options: Option<EventsOptions<String>>
    ) -> Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>>;
}

pub struct BollardDockerClient {
    inner: bollard::Docker,
}

impl BollardDockerClient {
    pub async fn new() -> Result<Self, DockerError> {
        let docker = bollard::Docker::connect_with_local_defaults()
            .map_err(|e| DockerError::ConnectionError {
                source: e,
                context: "Docker 데몬 연결 실패".to_string(),
            })?;
        
        Ok(Self { inner: docker })
    }
}

#[async_trait]
impl DockerClient for BollardDockerClient {
    async fn list_containers(
        &self, 
        options: Option<ListContainersOptions<String>>
    ) -> Result<Vec<ContainerSummary>, DockerError> {
        self.inner.list_containers(options)
            .await
            .map_err(|e| DockerError::ListContainersError {
                source: e,
                context: "컨테이너 목록 조회 실패".to_string(),
            })
    }

    fn events(
        &self, 
        options: Option<EventsOptions<String>>
    ) -> Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>> {
        let stream = self.inner.events(options)
            .map(|result| {
                result.map_err(|e| DockerError::ConnectionError {
                    source: e,
                    context: "Docker 이벤트 스트림 에러".to_string(),
                })
            });
        
        Box::pin(stream)
    }
}