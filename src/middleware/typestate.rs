use crate::settings::typestate::{TypeState, Raw, Validated, ValidationErrorCollector};
use std::fmt;

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
    use crate::settings::typestate::{Raw, Validated};

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
}