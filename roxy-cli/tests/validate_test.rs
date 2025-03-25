use std::fs;
use tempfile::tempdir;
use roxy_cli::commands::validate;

#[test]
fn test_validate_valid_config() -> anyhow::Result<()> {
    // 임시 디렉토리 생성
    let temp_dir = tempdir()?;
    let config_path = temp_dir.path().join("valid_config.json");
    
    // 유효한 설정 파일 작성
    let valid_config = r#"{
        "version": "1.0",
        "http": {
            "routers": {
                "api": {
                    "rule": "Host(`api.test`)",
                    "service": "api-service",
                    "middlewares": ["auth"]
                }
            },
            "services": {
                "api-service": {
                    "loadBalancer": {
                        "servers": [
                            {
                                "url": "http://localhost:8080"
                            }
                        ]
                    }
                }
            },
            "middlewares": {
                "auth": {
                    "type": "basicAuth",
                    "basicAuth": {
                        "users": ["user:password"]
                    }
                }
            }
        }
    }"#;
    
    fs::write(&config_path, valid_config)?;
    
    // 유효성 검사 실행 - 성공해야 함
    let result = validate::execute(&config_path);
    assert!(result.is_ok());
    
    Ok(())
}

#[test]
fn test_validate_invalid_config() -> anyhow::Result<()> {
    // 임시 디렉토리 생성
    let temp_dir = tempdir()?;
    let config_path = temp_dir.path().join("invalid_config.json");
    
    // 유효하지 않은 설정 파일 작성 (서비스 참조 누락)
    let invalid_config = r#"{
        "version": "1.0",
        "http": {
            "routers": {
                "api": {
                    "rule": "Host(`api.test`)",
                    "service": "non-existent-service",  // 존재하지 않는 서비스 참조
                    "middlewares": ["auth"]
                }
            },
            "middlewares": {
                "auth": {
                    "type": "basicAuth",
                    "basicAuth": {
                        "users": ["user:password"]
                    }
                }
            }
        }
    }"#;
    
    fs::write(&config_path, invalid_config)?;
    
    // 유효성 검사 실행 - 실패해야 함
    let result = validate::execute(&config_path);
    assert!(result.is_err());
    
    Ok(())
}

#[test]
fn test_validate_malformed_json() -> anyhow::Result<()> {
    // 임시 디렉토리 생성
    let temp_dir = tempdir()?;
    let config_path = temp_dir.path().join("malformed.json");
    
    // 잘못된 JSON 형식 작성
    let malformed_json = r#"{
        "version": "1.0",
        "http": {
            "routers": {
                "api": {
                    "rule": "Host(`api.test`)",
                    "service": "api-service",
                    "middlewares": ["auth"]
                }
            },
            // 이 주석은 JSON에서 유효하지 않음
            "missing_closing_brace": {
        }
    }"#;
    
    fs::write(&config_path, malformed_json)?;
    
    // 유효성 검사 실행 - 실패해야 함
    let result = validate::execute(&config_path);
    assert!(result.is_err());
    
    Ok(())
} 