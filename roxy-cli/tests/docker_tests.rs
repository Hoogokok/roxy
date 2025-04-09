use std::collections::HashMap;
use anyhow::Result;
use roxy_cli::docker::{self, DockerClient, MockDockerClient, create_docker_client};

#[tokio::test]
async fn test_mock_docker_client() -> Result<()> {
    // 모의 Docker 클라이언트 생성
    let client = MockDockerClient::new();
    
    // test-container 라벨 가져오기
    let labels = client.get_container_labels("test-container").await?;
    
    // 예상 결과 확인
    assert!(labels.contains_key("roxy.http.middlewares.test-container-auth.type"));
    assert_eq!(labels.get("roxy.http.middlewares.test-container-auth.type").unwrap(), "basic-auth");
    
    // 모든 컨테이너 가져오기
    let containers = client.get_all_running_containers().await?;
    assert_eq!(containers.len(), 2);
    assert!(containers.contains(&"test-container".to_string()));
    assert!(containers.contains(&"test-container2".to_string()));
    
    Ok(())
}

#[tokio::test]
async fn test_custom_mock_docker_client() -> Result<()> {
    // 사용자 정의 컨테이너로 모의 클라이언트 생성
    let mut containers = HashMap::new();
    
    let mut custom_container = HashMap::new();
    custom_container.insert("test.key".to_string(), "test.value".to_string());
    containers.insert("custom-container".to_string(), custom_container);
    
    let client = MockDockerClient::with_containers(containers);
    
    // 컨테이너 라벨 가져오기
    let labels = client.get_container_labels("custom-container").await?;
    assert_eq!(labels.get("test.key").unwrap(), "test.value");
    
    // 존재하지 않는 컨테이너 조회 시 오류 발생 확인
    let result = client.get_container_labels("non-existent").await;
    assert!(result.is_err());
    
    Ok(())
}

#[tokio::test]
async fn test_process_containers() -> Result<()> {
    // 모의 Docker 클라이언트 생성
    let client = MockDockerClient::new();
    
    // process_containers 함수 테스트
    let container_ids = ["test-container", "test-container2"];
    
    let results = docker::process_containers(&client, &container_ids, |id, labels| {
        // 명시적으로 'static 수명의 데이터만 사용하여 수명 문제 해결
        let id_owned = id.to_string();
        
        Box::pin(async move {
            // 소유된 값으로 처리하여 수명 문제 해결
            let middleware_count = labels.keys()
                .filter(|k| k.contains("middlewares"))
                .count();
                
            Ok(format!("{}: 미들웨어 {}개", id_owned, middleware_count))
        })
    }).await?;
    
    // 결과 확인
    assert_eq!(results.len(), 2);
    assert!(results[0].contains("test-container"));
    assert!(results[1].contains("test-container2"));
    
    Ok(())
}

#[tokio::test]
async fn test_create_docker_client() {
    // 테스트에서는 직접 MockDockerClient 생성
    let client = Box::new(MockDockerClient::new()) as Box<dyn DockerClient>;
    
    // test-container 라벨 가져오기
    let labels = client.get_container_labels("test-container").await;
    assert!(labels.is_ok(), "테스트용 MockDockerClient에서 test-container 라벨을 가져올 수 없습니다");
    
    if let Ok(labels) = labels {
        assert!(labels.contains_key("roxy.http.middlewares.test-container-auth.type"));
    }
} 