use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use roxy_cli::commands::migrate;
use roxy_cli::docker::{MockDockerClient, DockerClient};
use std::collections::HashMap;

#[tokio::test]
async fn test_validation_report_generation() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 테스트용 MockDockerClient 설정
    let mock_client = MockDockerClient::new();
    
    // 유효한 JSON 파일 생성
    let valid_json = r#"{
        "http": {
            "routers": {
                "test-router": {
                    "rule": "Host(`example.com`)",
                    "service": "test-service"
                }
            },
            "services": {
                "test-service": {
                    "loadbalancer": {
                        "servers": [
                            {"url": "http://localhost:8080"}
                        ]
                    }
                }
            }
        }
    }"#;
    
    let valid_file_path = output_dir.join("valid.json");
    fs::write(&valid_file_path, valid_json)?;
    
    // 유효하지 않은 JSON 파일 생성 (http 필드 없음)
    let invalid_json = r#"{
        "routers": {
            "test-router": {
                "rule": "Host(`example.com`)",
                "service": "test-service"
            }
        }
    }"#;
    
    let invalid_file_path = output_dir.join("invalid.json");
    fs::write(&invalid_file_path, invalid_json)?;
    
    // 마이그레이션 실행 (검증 활성화)
    let result = migrate::execute_with_validation(
        vec!["test-container".to_string()], 
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true,
        Box::new(mock_client) as Box<dyn DockerClient>
    ).await?;
    
    // 검증 보고서 확인
    assert!(result.validation_report.is_some());
    let report = result.validation_report.unwrap();
    
    // 보고서에 성공/실패 정보가 포함되어 있는지 확인
    assert!(report.contains("성공한 파일"));
    assert!(report.contains("유효한 JSON"));
    
    // invalid.json 파일의 문제점이 보고서에 포함되어 있는지 확인
    assert!(report.contains("invalid.json") || report.contains("'http' 필드가 없습니다"));
    
    Ok(())
} 