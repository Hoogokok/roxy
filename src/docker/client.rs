// src/docker/client.rs
use async_trait::async_trait;
use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage};
use bollard::system::EventsOptions;
use futures_util::Stream;

#[async_trait]
pub trait DockerClient {
    async fn list_containers(&self, options: Option<ListContainersOptions<String>>) 
        -> Result<Vec<ContainerSummary>, bollard::errors::Error>;
    
    async fn events(&self, options: Option<EventsOptions<String>>) 
        -> impl Stream<Item = Result<EventMessage, bollard::errors::Error>>;
}

// 실제 Docker 클라이언트 구현
pub struct BollardDockerClient(bollard::Docker);

#[async_trait]
impl DockerClient for BollardDockerClient {
    async fn list_containers(&self, options: Option<ListContainersOptions<String>>) 
        -> Result<Vec<ContainerSummary>, bollard::errors::Error> {
        self.0.list_containers(options).await
    }

    async fn events(&self, options: Option<EventsOptions<String>>) 
        -> impl Stream<Item = Result<EventMessage, bollard::errors::Error>> {
        self.0.events(options)
    }
}