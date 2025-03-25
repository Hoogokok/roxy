use std::collections::HashMap;
use anyhow::Result;
use std::sync::Arc;
use futures_util;

#[cfg(not(test))]
use bollard::Docker;
#[cfg(not(test))]
use bollard::container::ListContainersOptions;
#[cfg(not(test))]
use anyhow::Context;

// 함수 타입 정의
pub type GetContainerLabelsFn = Arc<dyn Fn(&str) -> futures_util::future::BoxFuture<'static, Result<HashMap<String, String>>> + Send + Sync>;
pub type GetAllRunningContainersFn = Arc<dyn Fn() -> futures_util::future::BoxFuture<'static, Result<Vec<String>>> + Send + Sync>;

pub struct DockerFunctions {
    pub get_container_labels: GetContainerLabelsFn,
    pub get_all_running_containers: GetAllRunningContainersFn,
}

// 기능별 모듈 분리
pub mod docker_provider {
    use super::*;

    // 실제 Docker 클라이언트 함수 생성 (프로덕션 환경)
    #[cfg(not(test))]
    pub fn create_functions() -> Result<DockerFunctions> {
        let docker = Docker::connect_with_local_defaults()
            .context("Docker 데몬에 연결할 수 없습니다")?;
        
        let docker_clone = docker.clone();
        let get_container_labels: GetContainerLabelsFn = Arc::new(move |container_id: &str| {
            let container_id = container_id.to_string();
            let docker = docker_clone.clone();
            Box::pin(async move {
                println!("실제 Docker 구현 사용 중");
                let container = docker.inspect_container(&container_id, None).await
                    .map_err(|e| anyhow::anyhow!("컨테이너 {}를 찾을 수 없습니다: {}", container_id, e))?;
                
                let labels = container.config
                    .and_then(|config| config.labels)
                    .unwrap_or_default();
                
                Ok(labels)
            })
        });
        
        let docker_clone = docker.clone();
        let get_all_running_containers: GetAllRunningContainersFn = Arc::new(move || {
            let docker = docker_clone.clone();
            Box::pin(async move {
                println!("실제 Docker 구현 사용 중");
                let options = Some(ListContainersOptions::<String>{
                    all: false,
                    ..Default::default()
                });
                
                let containers = docker.list_containers(options).await
                    .map_err(|e| anyhow::anyhow!("컨테이너 목록을 가져올 수 없습니다: {}", e))?;
                
                let container_ids = containers.iter()
                    .filter_map(|c| c.id.clone())
                    .collect();
                
                Ok(container_ids)
            })
        });
        
        Ok(DockerFunctions {
            get_container_labels,
            get_all_running_containers,
        })
    }

    // 테스트용 모의 Docker 함수 생성 (테스트 환경)
    #[cfg(test)]
    pub fn create_functions() -> Result<DockerFunctions> {
        let containers: HashMap<String, HashMap<String, String>> = {
            let mut containers = HashMap::new();
            
            // test-container 설정
            let mut test_container = HashMap::new();
            test_container.insert(
                "roxy.http.middlewares.test-container-auth.type".to_string(),
                "basic-auth".to_string(),
            );
            test_container.insert(
                "roxy.http.middlewares.test-container-auth.users".to_string(),
                "test:$apr1$H6uskkkW$IgXLP6ewTrSuBkTrqE8wj/".to_string(),
            );
            test_container.insert(
                "roxy.http.routers.test-container.rule".to_string(),
                "Host(`test.example.com`)".to_string(),
            );
            containers.insert("test-container".to_string(), test_container);
            
            // test-container2 설정
            let mut test_container2 = HashMap::new();
            test_container2.insert(
                "roxy.http.middlewares.test-container2-auth.type".to_string(),
                "basic-auth".to_string(),
            );
            test_container2.insert(
                "roxy.http.routers.test-container2.rule".to_string(),
                "Host(`test2.example.com`)".to_string(),
            );
            containers.insert("test-container2".to_string(), test_container2);
            
            containers
        };
        
        let containers_clone = containers.clone();
        let get_container_labels: GetContainerLabelsFn = Arc::new(move |container_id: &str| {
            let container_id = container_id.to_string();
            let containers = containers_clone.clone();
            Box::pin(async move {
                println!("테스트용 모의 Docker 구현 사용 중");
                match containers.get(&container_id) {
                    Some(labels) => Ok(labels.clone()),
                    None => Err(anyhow::anyhow!("컨테이너 {}를 찾을 수 없습니다", container_id))
                }
            })
        });
        
        let containers_clone = containers.clone();
        let get_all_running_containers: GetAllRunningContainersFn = Arc::new(move || {
            let containers = containers_clone.clone();
            Box::pin(async move {
                println!("테스트용 모의 Docker 구현 사용 중");
                Ok(containers.keys().cloned().collect())
            })
        });
        
        Ok(DockerFunctions {
            get_container_labels,
            get_all_running_containers,
        })
    }
}

// 공개 API
pub async fn get_container_labels(container_id: &str) -> Result<HashMap<String, String>> {
    let docker_functions = docker_provider::create_functions()?;
    (docker_functions.get_container_labels)(container_id).await
}

pub async fn get_all_running_containers() -> Result<Vec<String>> {
    let docker_functions = docker_provider::create_functions()?;
    (docker_functions.get_all_running_containers)().await
}

/// 예시 라벨 가져오기
pub fn get_example_labels() -> HashMap<String, String> {
    let mut labels = HashMap::new();
    
    // 미들웨어 예시
    labels.insert("roxy.http.middlewares.api-cors.type".to_string(), "cors".to_string());
    labels.insert("roxy.http.middlewares.api-cors.cors.allowOrigins".to_string(), 
                "http://localhost:3000,https://example.com".to_string());
    labels.insert("roxy.http.middlewares.api-cors.cors.allowMethods".to_string(), 
                "GET,POST,PUT,DELETE,OPTIONS".to_string());
    labels.insert("roxy.http.middlewares.api-cors.enabled".to_string(), "true".to_string());
    
    // 라우터 예시
    labels.insert("roxy.http.routers.api.rule".to_string(), 
                "Host(`test.localhost`) && PathPrefix(`/api`)".to_string());
    labels.insert("roxy.http.routers.api.middlewares".to_string(), "api-cors".to_string());
    labels.insert("roxy.http.routers.api.service".to_string(), "api".to_string());
    
    // 서비스 예시
    labels.insert("roxy.http.services.api.loadbalancer.server.port".to_string(), "80".to_string());
    
    labels
}
