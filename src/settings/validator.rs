use jsonschema::{
    JSONSchema,
    Draft,
    error::ValidationError as JSONValidationError,
};
use serde_json::{Value, from_str};
use tracing::debug;
use std::result::Result as StdResult;

use super::error::SettingsError;
use super::Result;

// schema.rs 모듈 가져오기
use super::schema::CONFIG_SCHEMA;

/// JSON 설정 검증을 위한 구조체
pub struct JsonConfigValidator {
    schema: JSONSchema,
}

/// JSON 유효성 검증 오류 타입
#[derive(Debug)]
pub enum ValidationError {
    ParseError(String),
    SchemaError { path: String, message: String },
    ReferenceError { path: String, reference: String, message: String },
}

impl JsonConfigValidator {
    /// 새 validator 인스턴스 생성 
    pub fn new() -> Result<Self> {
        debug!("JsonConfigValidator 초기화 중");
        
        // schema.rs에서 정의된 스키마 문자열 사용
        let schema_str = CONFIG_SCHEMA;
        
        // 스키마 파싱
        let schema_value: Value = from_str(schema_str)
            .map_err(|e| SettingsError::SchemaCompileError { 
                reason: format!("JSON 스키마 파싱 오류: {}", e) 
            })?;
            
        // 스키마 컴파일
        let schema = JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&schema_value)
            .map_err(|e| SettingsError::SchemaCompileError { 
                reason: format!("JSON 스키마 컴파일 오류: {}", e) 
            })?;
            
        debug!("JSON 스키마 컴파일 성공");
        Ok(Self { schema })
    }
    
    /// JSON 문자열 유효성 검사
    pub fn validate(&self, json_str: &str) -> StdResult<Value, Vec<ValidationError>> {
        // JSON 파싱
        let value: Value = match from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                return Err(vec![ValidationError::ParseError(
                    format!("Failed to parse JSON: {}", e)
                )]);
            }
        };
        
        // 스키마 검증
        let mut errors = Vec::new();
        let validation_errors: Vec<JSONValidationError> = self.schema
            .validate(&value)
            .map(|_| Vec::new())
            .unwrap_or_else(|e| e.into_iter().collect());
            
        for err in validation_errors {
            let path = err.instance_path.to_string();
            let message = err.to_string();
            errors.push(ValidationError::SchemaError { path, message });
        }
        
        // 참조 검증
        if let Err(ref_errors) = self.validate_references(&value) {
            errors.extend(ref_errors);
        }
        
        if errors.is_empty() {
            Ok(value)
        } else {
            Err(errors)
        }
    }
    
    /// 설정 객체의 참조 유효성 검사
    fn validate_references(&self, value: &Value) -> StdResult<(), Vec<ValidationError>> {
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
        let error_messages = errors.iter()
            .map(|e| match e {
                ValidationError::ParseError(msg) => format!("파싱 오류: {}", msg),
                ValidationError::SchemaError { path, message } => 
                    format!("스키마 오류 (경로: {}): {}", path, message),
                ValidationError::ReferenceError { path, reference, message } => 
                    format!("참조 오류 (경로: {}, 참조: {}): {}", path, reference, message),
            })
            .collect::<Vec<String>>();
            
        SettingsError::ValidationErrors { 
            errors: error_messages,
            file: String::from("unknown")
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