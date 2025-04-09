use anyhow::Result;
use tempfile::tempdir;
use std::collections::HashMap;
use roxy_cli::commands::migrate::MigrationContext;
use roxy_cli::docker::{MockDockerClient, DockerClient};

// 테스트용 Docker 클라이언트 생성 함수
fn create_test_docker_client() -> Box<dyn DockerClient> {
    Box::new(MockDockerClient::new())
}

#[tokio::test]
async fn test_migration_initialization() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    let backup_dir = temp_dir.path().join("backups");
    let log_file = temp_dir.path().join("migration.log");
    
    // 컨테이너 ID 설정
    let containers = vec!["test-container".to_string()];
    
    // 테스트용 Docker 클라이언트 설정
    let context = MigrationContext::new(
        Some(containers),
        false,
        output_dir.clone(),
        Some(backup_dir.clone()),
        Some(log_file.clone()),
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true,
        true,
        false,
    ).await?
    .with_test_docker_client(create_test_docker_client());
    
    // 초기화 및 검증
    let _context = context.initialize()?;
    
    // 로그 파일 생성 확인
    assert!(log_file.exists());
    
    Ok(())
}

#[tokio::test]
async fn test_migration_full_flow() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 테스트용 Docker 클라이언트 설정
    let context = MigrationContext::new(
        Some(vec!["test-container".to_string(), "test-container2".to_string()]),
        false,
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true,
        false,
        false,
    ).await?
    .with_test_docker_client(create_test_docker_client());
    
    let summary = context
        .initialize()?
        .validate().await?
        .backup()?
        .migrate().await?
        .get_summary()?;
    
    // 요약에 성공 메시지 포함 확인
    assert!(summary.contains("성공"));
    
    // 출력 파일 확인
    assert!(output_dir.join("test-container.json").exists());
    assert!(output_dir.join("test-container2.json").exists());
    
    Ok(())
} 