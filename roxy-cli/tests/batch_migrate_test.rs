use anyhow::Result;
use tempfile::tempdir;
use roxy_cli::commands::migrate;
use roxy_cli::docker::{MockDockerClient, DockerClient};

#[tokio::test]
async fn test_batch_migration_result_tracking() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 테스트용 MockDockerClient 설정
    let mock_client = MockDockerClient::new();
    
    // 테스트용 컨테이너 ID 설정 (하나는 성공, 하나는 실패할 예정)
    let containers = vec!["test-container".to_string(), "non-existent-container".to_string()];
    
    // 마이그레이션 실행
    let result = migrate::execute_batch_with_client(
        containers,
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        false,
        Box::new(mock_client) as Box<dyn DockerClient>
    ).await?;
    
    // 결과 검증
    assert_eq!(result.total, 2);
    assert_eq!(result.success, 1);
    assert_eq!(result.failed, 1);
    assert_eq!(result.failed_containers.len(), 1);
    assert!(result.failed_containers.contains(&"non-existent-container".to_string()));
    
    // 성공한 컨테이너에 대한 파일 생성 확인
    assert!(output_dir.join("test-container.json").exists());
    
    Ok(())
}
