use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;
use std::marker::PhantomData;
use crate::settings::typestate::{TypeState, Raw, Validated, Validatable, ValidationErrorCollector};
use crate::middleware::typestate::{MiddlewareValidator, MiddlewareConfigError};

/// Rate Limit 설정
#[derive(Debug, Clone)]
pub struct RateLimitConfig<S: TypeState = Raw> {
    /// 초당 평균 요청 수
    pub average: u32,
    
    /// 버스트 허용량
    pub burst: u32,
    
    /// 측정 기간
    pub period: Duration,
    
    /// 타입 상태 마커
    _state: PhantomData<S>,
}

impl RateLimitConfig<Raw> {
    /// 새로운 원시(Raw) 설정 생성
    pub fn new(average: u32, burst: u32) -> Self {
        Self {
            average,
            burst,
            period: Duration::from_secs(1),
            _state: PhantomData,
        }
    }
}

// validated 상태 설정에 대한 메서드
impl RateLimitConfig<Validated> {
    /// Docker 라벨에서 설정을 파싱하고 검증합니다.
    /// 파싱과 검증을 한 번에 수행하여 검증된 설정을 반환합니다.
    pub fn from_labels(labels: &HashMap<String, String>) -> Result<Self, String> {
        // 파싱: 값 추출하기
        let average = labels.get("rateLimit.average")
            .map(|v| v.parse::<u32>())
            .unwrap_or(Ok(100))
            .map_err(|_| "유효하지 않은 average 값".to_string())?;
            
        let burst = labels.get("rateLimit.burst")
            .map(|v| v.parse::<u32>())
            .unwrap_or(Ok(50))
            .map_err(|_| "유효하지 않은 burst 값".to_string())?;
            
        // 검증: 비즈니스 규칙 적용
        if average == 0 {
            return Err("average는 0보다 커야 합니다".to_string());
        }
        
        if burst == 0 {
            return Err("burst는 0보다 커야 합니다".to_string());
        }
        
        // 검증된 설정 생성
        Ok(Self {
            average,
            burst,
            period: Duration::from_secs(1),
            _state: PhantomData,
        })
    }
}

impl Validatable<RateLimitConfig<Validated>> for RateLimitConfig<Raw> {
    type Error = MiddlewareConfigError;
    
    fn validate(self) -> Result<RateLimitConfig<Validated>, Self::Error> {
        let mut validator = MiddlewareValidator::new();
        validator.start_collecting();
        
        // 평균 요청 수 검증
        if self.average == 0 {
            validator.add_error(MiddlewareConfigError::InvalidValue {
                field: "average".to_string(),
                message: "값은 0보다 커야 합니다".to_string(),
            });
        }
        
        // 버스트 값 검증
        if self.burst == 0 {
            validator.add_error(MiddlewareConfigError::InvalidValue {
                field: "burst".to_string(),
                message: "값은 0보다 커야 합니다".to_string(),
            });
        }
        
        // 검증 결과 처리
        match validator.into_result(()) {
            Ok(_) => Ok(RateLimitConfig {
                average: self.average,
                burst: self.burst,
                period: self.period,
                _state: PhantomData,
            }),
            Err(errors) => Err(errors[0].clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::get_value_case_insensitive;

    #[test]
    fn test_default_config() {
        let config = RateLimitConfig::<Raw> {
            average: 100,
            burst: 50,
            period: Duration::from_secs(1),
            _state: PhantomData,
        };

        assert_eq!(config.average, 100);
        assert_eq!(config.burst, 50);
        assert_eq!(config.period, Duration::from_secs(1));
    }

    #[test]
    fn test_from_labels_valid() {
        let mut labels = HashMap::new();
        labels.insert("rateLimit.average".to_string(), "200".to_string());
        labels.insert("rateLimit.burst".to_string(), "100".to_string());

        let config = RateLimitConfig::<Validated>::from_labels(&labels).unwrap();
        assert_eq!(config.average, 200);
        assert_eq!(config.burst, 100);
    }

    #[test]
    fn test_from_labels_invalid_value() {
        let mut labels = HashMap::new();
        labels.insert("rateLimit.average".to_string(), "invalid".to_string());

        let result = RateLimitConfig::<Validated>::from_labels(&labels);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("유효하지 않은 average 값"));
    }
    
    #[test]
    fn test_from_labels_validation_error() {
        let mut labels = HashMap::new();
        labels.insert("rateLimit.average".to_string(), "0".to_string());
        
        let result = RateLimitConfig::<Validated>::from_labels(&labels);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("average는 0보다 커야 합니다"));
    }

    #[test]
    fn test_from_labels_case_insensitive() {
        let mut labels = HashMap::new();
        // 다양한 케이스 스타일 테스트
        labels.insert("ratelimit.average".to_string(), "200".to_string());
        labels.insert("ratelimit.BURST".to_string(), "100".to_string());

        let config = RateLimitConfig::<Validated>::from_labels(&labels).unwrap();
        assert_eq!(config.average, 200);
        assert_eq!(config.burst, 100);
    }
    
    #[test]
    fn test_from_labels_snake_case() {
        let mut labels = HashMap::new();
        labels.insert("rate_limit.average".to_string(), "300".to_string());
        labels.insert("rate_limit.burst".to_string(), "150".to_string());

        let config = RateLimitConfig::<Validated>::from_labels(&labels).unwrap();
        assert_eq!(config.average, 300);
        assert_eq!(config.burst, 150);
    }
}