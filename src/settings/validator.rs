use std::collections::HashMap;
use std::path::Path;
use jsonschema::{JSONSchema, Draft};
use serde_json::Value;
use tracing::{error, debug};

use super::error::SettingsError;
use super::Result;
use super::json::JsonConfig;

/// JSON 설정 검증을 위한 구조체
pub struct JsonConfigValidator {
    schema: JSONSchema,
}

/// 검증 오류 타입
#[derive(Debug)]
pub enum ValidationError {
    ParseError(String),
    SchemaError { path: String, message: String },
    ReferenceError { path: String, reference: String, message: String },
}

impl JsonConfigValidator {
    /// 새로운 검증기 인스턴스 생성
    pub fn new() -> Result<Self> {
        // 인라인 기본 스키마 사용 (간단한 버전)
        let schema_str = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "version": {
                    "type": "string"
                }
            },
            "required": ["version"]
        }"#;
        
        let schema_value = match serde_json::from_str::<Value>(schema_str) {
            Ok(v) => v,
            Err(e) => {
                return Err(SettingsError::SchemaCompileError { 
                    reason: format!("스키마 파싱 오류: {}", e) 
                });
            }
        };
        
        // 스키마 컴파일
        let schema = match JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&schema_value) {
            Ok(s) => s,
            Err(e) => {
                return Err(SettingsError::SchemaCompileError { 
                    reason: format!("스키마 컴파일 오류: {}", e) 
                });
            }
        };
        
        Ok(Self { schema })
    }
    
    /// JSON 문자열 유효성 검사
    pub fn validate(&self, json_str: &str) -> std::result::Result<Value, Vec<ValidationError>> {
        // JSON 파싱
        let value = match serde_json::from_str::<Value>(json_str) {
            Ok(v) => v,
            Err(e) => {
                return Err(vec![ValidationError::ParseError(e.to_string())]);
            }
        };
        
        // 스키마 검증
        let result = self.schema.validate(&value);
        if let Err(errors) = result {
            let validation_errors = errors
                .map(|error| {
                    let path = error.instance_path.to_string();
                    ValidationError::SchemaError {
                        path,
                        message: error.to_string(),
                    }
                })
                .collect::<Vec<_>>();
            
            return Err(validation_errors);
        }
        
        // 참조 유효성 검사
        if let Err(errors) = self.validate_references(&value) {
            return Err(errors);
        }
        
        Ok(value.clone())
    }
    
    /// 설정 객체의 참조 유효성 검사
    fn validate_references(&self, value: &Value) -> std::result::Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        
        // 라우터 -> 서비스 참조 검증
        if let Some(routers) = value.get("routers").and_then(|v| v.as_object()) {
            let services = value.get("services")
                .and_then(|v| v.as_object())
                .map(|o| o.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            
            for (router_name, router) in routers {
                if let Some(service) = router.get("service").and_then(|v| v.as_str()) {
                    if !services.contains(&service.to_string()) {
                        errors.push(ValidationError::ReferenceError {
                            path: format!("routers.{}.service", router_name),
                            reference: service.to_string(),
                            message: format!("참조된 서비스 '{}' 없음", service),
                        });
                    }
                }
            }
        }
        
        // 라우터 -> 미들웨어 참조 검증
        if let Some(routers) = value.get("routers").and_then(|v| v.as_object()) {
            let middlewares = value.get("middlewares")
                .and_then(|v| v.as_object())
                .map(|o| o.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            
            for (router_name, router) in routers {
                if let Some(router_middlewares) = router.get("middlewares").and_then(|v| v.as_array()) {
                    for (i, middleware) in router_middlewares.iter().enumerate() {
                        if let Some(mw_name) = middleware.as_str() {
                            if !middlewares.contains(&mw_name.to_string()) {
                                errors.push(ValidationError::ReferenceError {
                                    path: format!("routers.{}.middlewares[{}]", router_name, i),
                                    reference: mw_name.to_string(),
                                    message: format!("참조된 미들웨어 '{}' 없음", mw_name),
                                });
                            }
                        }
                    }
                }
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl From<Vec<ValidationError>> for SettingsError {
    fn from(errors: Vec<ValidationError>) -> Self {
        let formatted_errors = errors.iter().map(|e| match e {
            ValidationError::ParseError(msg) => format!("파싱 오류: {}", msg),
            ValidationError::SchemaError { path, message } => 
                format!("스키마 오류 (경로: {}): {}", path, message),
            ValidationError::ReferenceError { path, reference, message } => 
                format!("참조 오류 (경로: {}, 참조: {}): {}", path, reference, message),
        }).collect();
        
        SettingsError::ValidationErrors { 
            errors: formatted_errors,
            file: "settings.json".to_string(), // 기본값, 실제로는 파일경로가 전달되어야 함
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validate_basic_json() {
        // 검증기 생성
        let validator = JsonConfigValidator::new().unwrap();
        
        // 유효한 JSON 검증
        let json = r#"{
            "version": "1.0"
        }"#;
        
        let result = validator.validate(json);
        assert!(result.is_ok());
        
        // 누락된 필수 필드가 있는 JSON 검증
        let invalid_json = r#"{
            "server": {
                "http_port": 80
            }
        }"#;
        
        let result = validator.validate(invalid_json);
        assert!(result.is_err());
    }
}