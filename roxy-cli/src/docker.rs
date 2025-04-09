use std::collections::HashMap;
use anyhow::Result;
use std::sync::Arc;
use futures_util::future::BoxFuture;
use async_trait::async_trait;

#[cfg(not(test))]
use bollard::Docker;
#[cfg(not(test))]
use bollard::container::ListContainersOptions;
#[cfg(not(test))]
use anyhow::Context;

// 1. 트레이트 정의: Docker 클라이언트 기능을 명시적 인터페이스로 정의
#[async_trait]
pub trait DockerClient: Send + Sync {
    async fn get_container_labels(&self, container_id: &str) -> Result<HashMap<String, String>>;
    async fn get_all_running_containers(&self) -> Result<Vec<String>>;
}

// 2. 기존 함수 타입 유지 (이전 코드와의 호환성을 위해)
pub type GetContainerLabelsFn = Arc<dyn Fn(&str) -> BoxFuture<'static, Result<HashMap<String, String>>> + Send + Sync>;
pub type GetAllRunningContainersFn = Arc<dyn Fn() -> BoxFuture<'static, Result<Vec<String>>> + Send + Sync>;

pub struct DockerFunctions {
    pub get_container_labels: GetContainerLabelsFn,
    pub get_all_running_containers: GetAllRunningContainersFn,
}

// 3. 실제 Docker 클라이언트 구현 (프로덕션 환경)
#[cfg(not(test))]
pub struct RealDockerClient {
    docker: bollard::Docker,
}

#[cfg(not(test))]
impl RealDockerClient {
    pub fn new() -> Result<Self> {
        let docker = bollard::Docker::connect_with_local_defaults()
            .map_err(|e| anyhow::anyhow!("Docker 데몬에 연결할 수 없습니다: {}", e))?;
        
        Ok(Self { docker })
    }
}

#[cfg(not(test))]
#[async_trait]
impl DockerClient for RealDockerClient {
    async fn get_container_labels(&self, container_id: &str) -> Result<HashMap<String, String>> {
        let container = self.docker.inspect_container(container_id, None).await
            .map_err(|e| anyhow::anyhow!("컨테이너 {}를 찾을 수 없습니다: {}", container_id, e))?;
        
        let labels = container.config
            .and_then(|config| config.labels)
            .unwrap_or_default();
        
        Ok(labels)
    }
    
    async fn get_all_running_containers(&self) -> Result<Vec<String>> {
        let options = Some(bollard::container::ListContainersOptions::<String>{
            all: false,
            ..Default::default()
        });
        
        let containers = self.docker.list_containers(options).await
            .map_err(|e| anyhow::anyhow!("컨테이너 목록을 가져올 수 없습니다: {}", e))?;
        
        let container_ids = containers.iter()
            .filter_map(|c| c.id.clone())
            .collect();
        
        Ok(container_ids)
    }
}

// 4. 모의 Docker 클라이언트 구현
#[derive(Clone)]
pub struct MockDockerClient {
    containers: HashMap<String, HashMap<String, String>>,
}

impl MockDockerClient {
    pub fn new() -> Self {
        Self { containers: Self::create_mock_containers() }
    }
    
    fn create_mock_containers() -> HashMap<String, HashMap<String, String>> {
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
    }
    
    // 사용자 정의 컨테이너 데이터로 모의 클라이언트 생성
    pub fn with_containers(containers: HashMap<String, HashMap<String, String>>) -> Self {
        Self { containers }
    }
}

#[async_trait]
impl DockerClient for MockDockerClient {
    async fn get_container_labels(&self, container_id: &str) -> Result<HashMap<String, String>> {
        println!("테스트용 모의 Docker 구현에서 컨테이너 {} 라벨 조회 시도", container_id);
        match self.containers.get(container_id) {
            Some(labels) => {
                println!("컨테이너 {} 라벨 {}개 찾음", container_id, labels.len());
                Ok(labels.clone())
            }
            None => {
                println!("컨테이너 {}를 찾을 수 없습니다", container_id);
                Err(anyhow::anyhow!("컨테이너 {}를 찾을 수 없습니다", container_id))
            }
        }
    }
    
    async fn get_all_running_containers(&self) -> Result<Vec<String>> {
        println!("테스트용 모의 Docker 구현에서 모든 컨테이너 목록 조회");
        let container_ids = self.containers.keys().cloned().collect();
        println!("컨테이너 {}개 찾음", self.containers.len());
        Ok(container_ids)
    }
}

// 5. 기존 함수형 구현을 유지하되, 트레이트 기반 구현을 사용하도록 수정
pub mod docker_provider {
    use super::*;

    #[cfg(not(test))]
    pub fn create_functions() -> Result<DockerFunctions> {
        // 실제 Docker 클라이언트 생성
        let client = Arc::new(RealDockerClient::new()?);
        
        // 기존 함수 타입과 호환되는 클로저 생성
        let client_clone = Arc::clone(&client);
        let get_container_labels: GetContainerLabelsFn = Arc::new(move |container_id: &str| {
            let client = Arc::clone(&client_clone);
            let container_id = container_id.to_string();
            Box::pin(async move {
                client.get_container_labels(&container_id).await
            })
        });
        
        let client_clone = Arc::clone(&client);
        let get_all_running_containers: GetAllRunningContainersFn = Arc::new(move || {
            let client = Arc::clone(&client_clone);
            Box::pin(async move {
                client.get_all_running_containers().await
            })
        });
        
        Ok(DockerFunctions {
            get_container_labels,
            get_all_running_containers,
        })
    }

    #[cfg(test)]
    pub fn create_functions() -> Result<DockerFunctions> {
        // 모의 Docker 클라이언트 생성
        let client = Arc::new(MockDockerClient::new());
        
        // 기존 함수 타입과 호환되는 클로저 생성
        let client_clone = Arc::clone(&client);
        let get_container_labels: GetContainerLabelsFn = Arc::new(move |container_id: &str| {
            let client = Arc::clone(&client_clone);
            let container_id = container_id.to_string();
            Box::pin(async move {
                client.get_container_labels(&container_id).await
            })
        });
        
        let client_clone = Arc::clone(&client);
        let get_all_running_containers: GetAllRunningContainersFn = Arc::new(move || {
            let client = Arc::clone(&client_clone);
            Box::pin(async move {
                client.get_all_running_containers().await
            })
        });
        
        Ok(DockerFunctions {
            get_container_labels,
            get_all_running_containers,
        })
    }
}

// 6. 고차 함수 추가
pub async fn with_container_labels<F, T>(
    client: &dyn DockerClient,
    container_id: &str,
    f: F
) -> Result<T>
where
    F: FnOnce(&HashMap<String, String>) -> T,
{
    let labels = client.get_container_labels(container_id).await?;
    Ok(f(&labels))
}

pub async fn process_containers<F, T>(
    client: &dyn DockerClient,
    container_ids: &[&str],
    f: F
) -> Result<Vec<T>>
where
    F: for<'a> Fn(&str, &'a HashMap<String, String>) -> BoxFuture<'a, Result<T>> + Send + Sync,
{
    let mut results = Vec::with_capacity(container_ids.len());
    
    for &id in container_ids {
        let labels = client.get_container_labels(id).await?;
        let result = f(id, &labels).await?;
        results.push(result);
    }
    
    Ok(results)
}

// 7. 공개 API - 이전 API 유지
pub async fn get_container_labels(container_id: &str) -> Result<HashMap<String, String>> {
    let docker_functions = docker_provider::create_functions()?;
    (docker_functions.get_container_labels)(container_id).await
}

pub async fn get_all_running_containers() -> Result<Vec<String>> {
    let docker_functions = docker_provider::create_functions()?;
    (docker_functions.get_all_running_containers)().await
}

// 8. 새로운 클라이언트 생성 함수 추가
pub fn create_docker_client() -> Box<dyn DockerClient> {
    // 테스트 환경 감지를 위한 환경 변수 확인
    #[cfg(test)]
    {
        return Box::new(MockDockerClient::new());
    }

    // 프로덕션 환경
    #[cfg(not(test))]
    {
        match RealDockerClient::new() {
            Ok(client) => Box::new(client),
            Err(e) => {
                eprintln!("Docker 클라이언트 생성 오류: {}", e);
                // Docker 데몬 문제 시 빈 클라이언트로 폴백
                Box::new(MockDockerClient::with_containers(HashMap::new()))
            }
        }
    }
}

// 테스트용 클라이언트 생성 - 테스트에서 직접 호출하기 위한 함수
#[cfg(test)]
pub fn create_test_docker_client() -> Box<dyn DockerClient> {
    Box::new(MockDockerClient::new())
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
