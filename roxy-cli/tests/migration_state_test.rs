use anyhow::Result;
use std::path::PathBuf;
use tempfile::tempdir;
use roxy_cli::commands::migrate::{MigrationContext, Uninitialized, Initialized, Validated};
use roxy_cli::docker::MockDockerClient;

#[tokio::test]
async fn test_migration_state_transitions() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 초기 상태 생성
    let uninitialized = MigrationContext::<Uninitialized>::new(
        Some(vec!["test-container".to_string()]),
        false,
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true,
        false,
        false,
    ).await?;
    
    // 테스트용 Docker 클라이언트 설정
    let docker_client = Box::new(MockDockerClient::new());
    let uninitialized = uninitialized.with_test_docker_client(docker_client);
    
    // 상태 전환: Uninitialized -> Initialized
    let initialized = uninitialized.initialize()?;
    
    // Uninitialized 상태의 메서드는 더 이상 호출할 수 없음 (컴파일 오류)
    // uninitialized.initialize(); // 컴파일 오류!
    
    // 상태 전환: Initialized -> Validated
    let validated = initialized.validate().await?;
    
    // Initialized 상태의 메서드는 더 이상 호출할 수 없음
    // initialized.validate().await; // 컴파일 오류!
    
    // 타입 시스템이 상태 전환을 강제하는지 확인
    // 이 시점에서는 validated.migrate()를 직접 호출할 수 없음
    // validated.migrate().await; // 컴파일 오류!
    
    // 백업 단계를 거쳐야만 마이그레이션 가능
    let _backed = validated.backup()?;
    
    Ok(())
}

#[tokio::test]
async fn test_validation_failure_handling() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 존재하지 않는 컨테이너로 설정
    let containers = vec!["non-existent-container".to_string()];
    
    // fail_fast를 false로 설정하여 오류가 있어도 계속 진행
    let uninitialized = MigrationContext::<Uninitialized>::new(
        Some(containers),
        false,
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true,
        false, // fail_fast: false
        false,
    ).await?;
    
    // 모의 Docker 클라이언트 설정
    let docker_client = Box::new(MockDockerClient::new());
    let uninitialized = uninitialized.with_test_docker_client(docker_client);
    
    // 초기화
    let initialized = uninitialized.initialize()?;
    
    // 검증 단계 실행 - 여기서 오류가 발생해도 fail_fast가 false이므로 진행됨
    let validation_result = initialized.validate().await;
    assert!(validation_result.is_ok(), "fail_fast가 false일 때 검증 실패가 무시되어야 함");
    
    Ok(())
} 