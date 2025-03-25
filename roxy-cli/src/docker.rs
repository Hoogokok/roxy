use anyhow::{Context, Result};
use std::collections::HashMap;
use bollard::Docker;
use bollard::container::ListContainersOptions;

/// 실제 컨테이너 라벨 가져오기
pub async fn get_container_labels(container_id: &str) -> Result<HashMap<String, String>> {
    let docker = Docker::connect_with_local_defaults()
        .context("Docker 데몬에 연결할 수 없습니다")?;
    
    let container = docker.inspect_container(container_id, None).await
        .context(format!("컨테이너 {}를 찾을 수 없습니다", container_id))?;
    
    // 컨테이너 라벨 추출
    let labels = container.config
        .and_then(|config| config.labels)
        .unwrap_or_default();
    
    Ok(labels)
}

/// 모든 실행 중인 컨테이너 ID 가져오기
pub async fn get_all_running_containers() -> Result<Vec<String>> {
    let docker = Docker::connect_with_local_defaults()
        .context("Docker 데몬에 연결할 수 없습니다")?;
    
    let options = Some(ListContainersOptions::<String>{
        all: false, // 실행 중인 컨테이너만
        ..Default::default()
    });
    
    let containers = docker.list_containers(options).await
        .context("컨테이너 목록을 가져올 수 없습니다")?;
    
    let container_ids = containers.iter()
        .filter_map(|c| c.id.clone())
        .collect();
    
    Ok(container_ids)
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
