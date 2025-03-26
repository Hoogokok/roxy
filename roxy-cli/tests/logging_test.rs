use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use roxy_cli::commands::migrate;
use roxy_cli::docker::{MockDockerClient, DockerClient};

#[tokio::test]
async fn test_migration_with_enhanced_logging() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    let log_file = temp_dir.path().join("detailed.log");
    
    // 테스트용 MockDockerClient 설정
    let mock_client = MockDockerClient::new();
    
    // 마이그레이션 실행
    let result = migrate::execute_with_detailed_logging(
        vec!["test-container".to_string()],
        output_dir.clone(),
        None,
        Some(log_file.clone()),
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true,
        Box::new(mock_client) as Box<dyn DockerClient>
    ).await?;
    
    // 로그 파일 존재 확인
    assert!(log_file.exists());
    
    // 로그 내용 검증
    let log_content = fs::read_to_string(log_file)?;
    assert!(log_content.contains("마이그레이션 시작"));
    assert!(log_content.contains("라벨 개수"));
    assert!(log_content.contains("변환된 JSON 크기"));
    
    // 결과 검증
    assert_eq!(result.batch_result.total, 1);
    assert_eq!(result.batch_result.success, 1);
    assert_eq!(result.batch_result.failed, 0);
    
    // 출력 파일 확인
    let output_file = output_dir.join("test-container.json");
    assert!(output_file.exists());
    
    Ok(())
}

#[tokio::test]
async fn test_migration_with_backup() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    let backup_dir = temp_dir.path().join("backups");
    
    // 테스트용 MockDockerClient 설정
    let mock_client = MockDockerClient::new();
    
    // 첫 번째 마이그레이션 실행
    migrate::execute_with_detailed_logging(
        vec!["test-container".to_string()],
        output_dir.clone(),
        Some(backup_dir.clone()),
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        false,
        Box::new(mock_client.clone()) as Box<dyn DockerClient>
    ).await?;
    
    // 첫 번째 출력 파일 확인
    let output_file = output_dir.join("test-container.json");
    assert!(output_file.exists());
    
    // 첫 번째 출력 파일 내용 저장
    let first_content = fs::read_to_string(&output_file)?;
    
    // 두 번째 마이그레이션 실행 (자동 백업 확인용)
    migrate::execute_with_detailed_logging(
        vec!["test-container".to_string()],
        output_dir.clone(),
        Some(backup_dir.clone()),
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        false,
        Box::new(mock_client) as Box<dyn DockerClient>
    ).await?;
    
    // 백업 디렉토리 확인
    assert!(backup_dir.exists());
    
    // 백업 파일 존재 확인
    let backup_files: Vec<_> = fs::read_dir(&backup_dir)?
        .filter_map(|e| e.ok())
        .collect();
    
    assert!(!backup_files.is_empty(), "백업 파일이 생성되지 않았습니다");
    
    // 백업 파일 내용 확인
    let backup_file = &backup_files[0].path();
    let backup_content = fs::read_to_string(backup_file)?;
    
    assert_eq!(first_content, backup_content, "백업 파일 내용이 원본과 다릅니다");
    
    Ok(())
} 