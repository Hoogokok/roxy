use std::fmt;
use std::time::Duration;

use crate::settings::typestate::ValidationErrorCollector;

/// 미들웨어 설정 오류
#[derive(Debug, Clone)]
pub enum MiddlewareConfigError {
    InvalidValue {
        field: String,
        message: String,
    },
    MissingField {
        field: String,
    },
    ParseError(String),
}

impl fmt::Display for MiddlewareConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue { field, message } => {
                write!(f, "유효하지 않은 설정 값: {} - {}", field, message)
            }
            Self::MissingField { field } => {
                write!(f, "필수 필드 누락: {}", field)
            }
            Self::ParseError(msg) => {
                write!(f, "설정 파싱 오류: {}", msg)
            }
        }
    }
}

impl std::error::Error for MiddlewareConfigError {}

/// 미들웨어 설정 검증기
pub struct MiddlewareValidator {
    errors: Vec<MiddlewareConfigError>,
    collecting: bool,
}

impl MiddlewareValidator {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            collecting: false,
        }
    }
    
    /// 수집된 오류 목록 반환 및 소유권 이전
    pub fn into_errors(self) -> Vec<MiddlewareConfigError> {
        self.errors
    }
    
    /// 검증 결과를 안전하게 처리하는 메서드
    pub fn into_result<T>(self, value: T) -> Result<T, Vec<MiddlewareConfigError>> {
        if self.has_errors() {
            Err(self.errors)
        } else {
            Ok(value)
        }
    }
}


impl ValidationErrorCollector for MiddlewareValidator {
    type Error = MiddlewareConfigError;
    
    fn start_collecting(&mut self) {
        self.collecting = true;
    }
    
    fn add_error(&mut self, error: Self::Error) {
        if self.collecting {
            self.errors.push(error);
        }
    }
    
    fn get_errors(&self) -> Vec<&Self::Error> {
        self.errors.iter().collect()
    }
    
    fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    
    fn handle_errors<T>(&self) -> Result<T, Vec<Self::Error>> {
        Err(self.errors.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{middleware::rate_limit::{store::RateLimitStore, RateLimitConfig, RateLimitMiddleware}, settings::typestate::{Raw, TypeState, Validatable, Validated}};
    use std::marker::PhantomData;

    #[test]
    fn test_middleware_validator_collects_errors() {
        let mut validator = MiddlewareValidator::new();
        validator.start_collecting();
        
        // 오류 추가
        validator.add_error(MiddlewareConfigError::InvalidValue {
            field: "test_field".to_string(),
            message: "테스트 오류".to_string(),
        });
        
        // 오류가 제대로 수집되었는지 확인
        assert!(validator.has_errors());
        assert_eq!(validator.get_errors().len(), 1);
        
        // 오류 내용 확인
        let errors = validator.into_errors();
        match &errors[0] {
            MiddlewareConfigError::InvalidValue { field, message } => {
                assert_eq!(field, "test_field");
                assert_eq!(message, "테스트 오류");
            },
            _ => panic!("잘못된 오류 타입"),
        }
    }
    
    #[test]
    fn test_middleware_validator_multiple_errors() {
        let mut validator = MiddlewareValidator::new();
        validator.start_collecting();
        
        // 여러 오류 추가
        validator.add_error(MiddlewareConfigError::InvalidValue {
            field: "field1".to_string(),
            message: "오류1".to_string(),
        });
        
        validator.add_error(MiddlewareConfigError::MissingField {
            field: "field2".to_string(),
        });
        
        // 오류 수 확인
        assert_eq!(validator.get_errors().len(), 2);
        
        // handle_errors 메서드 테스트
        let result: Result<(), Vec<MiddlewareConfigError>> = 
            if validator.has_errors() {
                Err(validator.into_errors())
            } else {
                Ok(())
            };
        
        assert!(result.is_err());
        let errors = result.err().unwrap();
        assert_eq!(errors.len(), 2);
    }
    
    #[test]
    fn test_handle_errors_always_returns_errors() {
        // 오류가 있는 경우
        let mut validator = MiddlewareValidator::new();
        validator.start_collecting();
        validator.add_error(MiddlewareConfigError::InvalidValue {
            field: "test".to_string(),
            message: "테스트 오류".to_string(),
        });
        
        let result: Result<(), _> = validator.handle_errors();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().len(), 1);
        
        // 오류가 없는 경우에도 빈 오류 목록 반환
        let validator = MiddlewareValidator::new();
        let result: Result<(), _> = validator.handle_errors();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().len(), 0);
    }
    
    #[test]
    fn test_into_result() {
        // 오류가 있는 경우
        let mut validator = MiddlewareValidator::new();
        validator.start_collecting();
        validator.add_error(MiddlewareConfigError::InvalidValue {
            field: "test".to_string(),
            message: "테스트 오류".to_string(),
        });
        
        let result = validator.into_result(42);
        assert!(result.is_err());
        
        // 오류가 없는 경우
        let validator = MiddlewareValidator::new();
        let result = validator.into_result("성공");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "성공");
    }
    
    #[test]
    fn test_middleware_error_display() {
        let error1 = MiddlewareConfigError::InvalidValue { 
            field: "필드명".to_string(), 
            message: "값이 잘못됨".to_string() 
        };
        let error2 = MiddlewareConfigError::MissingField { 
            field: "필수필드".to_string() 
        };
        let error3 = MiddlewareConfigError::ParseError("파싱 실패".to_string());
        
        assert_eq!(
            format!("{}", error1),
            "유효하지 않은 설정 값: 필드명 - 값이 잘못됨"
        );
        assert_eq!(
            format!("{}", error2),
            "필수 필드 누락: 필수필드"
        );
        assert_eq!(
            format!("{}", error3),
            "설정 파싱 오류: 파싱 실패"
        );
    }

    // 미들웨어 설정을 위한 타입스테이트 테스트
    #[derive(Debug, Clone)]
    struct TestMiddlewareConfig<S: TypeState = Raw> {
        name: String,
        value: i32,
        _state: PhantomData<S>,
    }

    impl TestMiddlewareConfig<Raw> {
        fn new(name: &str, value: i32) -> Self {
            Self {
                name: name.to_string(),
                value,
                _state: PhantomData,
            }
        }
    }

    impl Validatable<TestMiddlewareConfig<Validated>> for TestMiddlewareConfig<Raw> {
        type Error = MiddlewareConfigError;

        fn validate(self) -> Result<TestMiddlewareConfig<Validated>, Self::Error> {
            let mut validator = MiddlewareValidator::new();
            validator.start_collecting();
            
            // 이름 검증
            if self.name.is_empty() {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: "name".to_string(),
                    message: "이름은 비어있을 수 없습니다".to_string(),
                });
            }
            
            // 값 검증
            if self.value <= 0 {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: "value".to_string(),
                    message: "값은 0보다 커야 합니다".to_string(),
                });
            }
            
            // 검증 결과 처리
            match validator.into_result(()) {
                Ok(_) => Ok(TestMiddlewareConfig {
                    name: self.name,
                    value: self.value,
                    _state: PhantomData,
                }),
                Err(errors) => Err(errors[0].clone()),
            }
        }
    }

    #[test]
    fn test_middleware_typestate_validation_success() {
        // 유효한 설정으로 테스트
        let raw_config = TestMiddlewareConfig::new("test", 42);
        let validated_result = raw_config.validate();
        
        assert!(validated_result.is_ok());
        let validated = validated_result.unwrap();
        assert_eq!(validated.name, "test");
        assert_eq!(validated.value, 42);
    }

    #[test]
    fn test_middleware_typestate_validation_failure() {
        // 유효하지 않은 설정으로 테스트
        let raw_config = TestMiddlewareConfig::new("", -5);
        let validated_result = raw_config.validate();
        
        assert!(validated_result.is_err());
        let error = validated_result.unwrap_err();
        
        match error {
            MiddlewareConfigError::InvalidValue { field, .. } => {
                // 첫 번째 오류가 name 또는 value 필드에 관련된 것인지 확인
                assert!(field == "name" || field == "value");
            },
            _ => panic!("예상치 못한 오류 타입"),
        }
    }

    #[test]
    fn test_rate_limit_config_validation_success() {
        // 유효한 값으로 설정
        let raw_config = RateLimitConfig::<Raw>::new(100, 60);
        let validated_result = raw_config.validate();
        
        assert!(validated_result.is_ok());
        let validated = validated_result.unwrap();
        assert_eq!(validated.average, 100);
        assert_eq!(validated.burst, 60);
    }

    #[test]
    fn test_rate_limit_config_validation_failure() {
        // 유효하지 않은 값으로 설정
        let raw_config = RateLimitConfig::<Raw>::new(0, 0);
        let validated_result = raw_config.validate();
        
        assert!(validated_result.is_err());
        // 특정 오류 타입과 메시지 검증
        match validated_result.unwrap_err() {
            MiddlewareConfigError::InvalidValue { field, message } => {
                assert!(field == "average" || field == "burst");
                assert!(message.contains("0보다 커야 합니다"));
            },
            _ => panic!("예상치 못한 오류 타입"),
        }
    }
    
    // 테스트 모듈에 추가할 모의 구현
    #[derive(Default)]
    struct MockRateLimitStore;

    #[async_trait::async_trait]
    impl RateLimitStore for MockRateLimitStore {
        async fn check_rate(&self, _key: &str, _rate: f64, _burst: f64) -> bool {
            true
        }
        
        async fn time_to_next_request(&self, _key: &str) -> Option<Duration> {
            None
        }
    }

    // 테스트 수정
    #[test]
    fn test_rate_limit_middleware_requires_validated_config() {
        let validated_config = RateLimitConfig::<Raw>::new(100, 60)
            .validate()
            .expect("설정 검증에 실패했습니다");
        
        let mock_store = MockRateLimitStore;
        let middleware = RateLimitMiddleware::new(validated_config, mock_store);
        assert_eq!(middleware.config.average, 100);
    }
}