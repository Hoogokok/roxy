use std::future::Future;

/// 검증 상태를 나타내는 마커 트레이트
pub trait TypeState {}

/// 원시 상태 (검증되지 않음)
#[derive(Debug, Clone)]
pub struct Raw {}

/// 검증된 상태
#[derive(Debug, Clone)]
pub struct Validated {}

impl TypeState for Raw {}
impl TypeState for Validated {}

/// 검증 가능한 타입에 대한 트레이트
pub trait Validatable<T> {
    type Error;
    fn validate(self) -> Result<T, Self::Error>;
}

/// 비동기 검증을 위한 트레이트
pub trait AsyncValidatable<T> {
    type Error;
    fn validate_async(self) -> impl Future<Output = Result<T, Self::Error>> + Send
    where
        Self: Send;
}

/// 동기 검증 가능한 타입을 비동기 검증으로 확장
/// 이를 통해 동기 검증 타입은 기본적으로 비동기 검증도 지원
impl<T, V> AsyncValidatable<T> for V 
where 
    V: Validatable<T> + Send,
    V::Error: Send,
{
    type Error = V::Error;
    
    fn validate_async(self) -> impl Future<Output = Result<T, Self::Error>> + Send {
        async move {
            self.validate() // 기본 구현은 동기 validate 호출
        }
    }
}

/// 타입 변환 문맥에서 사용 가능한 검증
pub trait ContextValidatable<T, C> {
    type Error;
    fn validate_with_context(self, context: &C) -> Result<T, Self::Error>;
}

/// 컨텍스트 기반 비동기 검증을 위한 트레이트
pub trait AsyncContextValidatable<T, C> {
    type Error;
    fn validate_with_context_async(self, context: &C) -> impl Future<Output = Result<T, Self::Error>> + Send
    where
        Self: Send,
        C: Sync;
}

/// 검증 오류를 수집하는 트레이트
pub trait ValidationErrorCollector {
    type Error;
    
    /// 오류 수집 시작
    fn start_collecting(&mut self);
    
    /// 오류 추가
    fn add_error(&mut self, error: Self::Error);
    
    /// 수집된 오류 반환
    fn get_errors(&self) -> Vec<&Self::Error>;
    
    /// 오류가 있는지 확인
    fn has_errors(&self) -> bool;
    
    /// 수집된 오류 처리
    fn handle_errors<T>(&self) -> Result<T, Vec<Self::Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::marker::PhantomData;

    // 테스트를 위한 간단한 설정 구조체
    #[derive(Debug)]
    struct TestConfig<S: TypeState = Validated> {
        value: i32,
        _marker: PhantomData<S>,
    }

    impl TestConfig<Raw> {
        fn new(value: i32) -> Self {
            Self {
                value,
                _marker: PhantomData,
            }
        }
    }

    impl Validatable<TestConfig<Validated>> for TestConfig<Raw> {
        type Error = &'static str;

        fn validate(self) -> Result<TestConfig<Validated>, Self::Error> {
            if self.value < 0 {
                return Err("Value must be positive");
            }

            Ok(TestConfig {
                value: self.value,
                _marker: PhantomData,
            })
        }
    }

    #[test]
    fn test_validation_chain() {
        let raw = TestConfig::<Raw>::new(42);
        let validated = raw.validate().and_then(|v| Ok(v));
        assert!(validated.is_ok());

        let raw_invalid = TestConfig::<Raw>::new(-10);
        let validated_invalid = raw_invalid.validate();
        assert!(validated_invalid.is_err());
    }

    #[tokio::test]
    async fn test_async_validation() {
        let raw = TestConfig::<Raw>::new(42);
        let validated = raw.validate_async().await;
        assert!(validated.is_ok());
    }
}