use anyhow::Result;
use tempfile::tempdir;
use roxy_cli::commands::migrate;
use roxy_cli::docker::{MockDockerClient, DockerClient};
use std::fs;

#[tokio::test]
async fn test_migration_validation_report() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 테스트용 MockDockerClient 설정
    let mock_client = MockDockerClient::new();
    
    // 테스트용 컨테이너 ID 설정
    let containers = vec!["test-container".to_string()];
    
    // 검증 기능이 포함된 마이그레이션 실행
    let result = migrate::execute_with_validation(
        containers,
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true, // 검증 활성화
        Box::new(mock_client) as Box<dyn DockerClient>
    ).await?;
    
    // 검증 결과 확인
    assert!(result.validation_report.is_some());
    let report = result.validation_report.unwrap();
    
    // 보고서에 필요한 정보가 포함되어 있는지 확인
    assert!(report.contains("검증 결과"));
    assert!(report.contains("성공한 파일"));
    assert!(report.contains("유효한 JSON"));
    
    // 생성된 파일 확인
    assert!(fs::metadata(output_dir.join("test-container.json")).is_ok());
    
    Ok(())
}
