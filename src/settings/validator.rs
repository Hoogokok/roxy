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
    /// 새 validator 인스턴스 생성 
    pub fn new() -> Result<Self> {
        // 내장 JSON 스키마 정의
        let schema_str = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["version"],
            "properties": {
                "version": {
                    "type": "string",
                    "enum": ["1.0"]
                },
                "id": {
                    "type": "string"
                },
                "server": {
                    "type": "object",
                    "properties": {
                        "http_port": {"type": "integer", "minimum": 1, "maximum": 65535},
                        "https_port": {"type": "integer", "minimum": 1, "maximum": 65535},
                        "https_enabled": {"type": "boolean"},
                        "retry_count": {"type": "integer", "minimum": 0},
                        "retry_interval": {"type": "integer", "minimum": 0}
                    }
                },
                "middlewares": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "required": ["type"],
                        "properties": {
                            "type": {
                                "type": "string",
                                "enum": ["basic-auth", "cors", "ratelimit", "headers", "compress"]
                            },
                            "users": {
                                "type": "array",
                                "items": {"type": "string"}
                            },
                            "allow_origins": {
                                "type": "array",
                                "items": {"type": "string"}
                            },
                            "allow_methods": {
                                "type": "array",
                                "items": {"type": "string"}
                            },
                            "average": {"type": "integer", "minimum": 0},
                            "burst": {"type": "integer", "minimum": 0},
                            "headers": {
                                "type": "object",
                                "additionalProperties": {"type": "string"}
                            }
                        }
                    }
                },
                "routers": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "required": ["rule", "service"],
                        "properties": {
                            "rule": {"type": "string"},
                            "service": {"type": "string"},
                            "middlewares": {
                                "type": "array",
                                "items": {"type": "string"}
                            },
                            "priority": {"type": "integer"}
                        }
                    }
                },
                "services": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "required": ["loadbalancer"],
                        "properties": {
                            "loadbalancer": {
                                "type": "object",
                                "required": ["servers"],
                                "properties": {
                                    "servers": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "required": ["url"],
                                            "properties": {
                                                "url": {"type": "string", "format": "uri"},
                                                "weight": {"type": "integer", "minimum": 1}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "health": {
                    "type": "object",
                    "properties": {
                        "enabled": {"type": "boolean"},
                        "interval": {"type": "integer", "minimum": 1},
                        "timeout": {"type": "integer", "minimum": 1},
                        "max_failures": {"type": "integer", "minimum": 0},
                        "http": {
                            "type": "object",
                            "properties": {
                                "path": {"type": "string"}
                            }
                        }
                    }
                },
                "router_middlewares": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "array",
                        "items": {"type": "string"}
                    }
                }
            }
        }"#;
        
        // 스키마 컴파일
        let schema_value: Value = serde_json::from_str(schema_str)
            .map_err(|e| SettingsError::SchemaCompileError { 
                reason: format!("스키마 파싱 오류: {}", e) 
            })?;
            
        let schema = JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&schema_value)
            .map_err(|e| SettingsError::SchemaCompileError { 
                reason: format!("스키마 컴파일 오류: {}", e) 
            })?;
            
        debug!("JSON 스키마 컴파일 성공");
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