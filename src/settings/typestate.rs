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

/// 부분 검증을 위한 트레이트
pub trait PartialValidatable<T> {
    type Error;
    
    /// 특정 필드만 검증
    fn validate_field<F>(&self, field: F) -> Result<(), Self::Error>
    where
        F: AsRef<str>;
    
    /// 지정된 필드들만 검증
    fn validate_fields<I, F>(&self, fields: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = F>,
        F: AsRef<str>,
    {
        for field in fields {
            self.validate_field(field)?;
        }
        Ok(())
    }
}

/// 부분 비동기 검증을 위한 트레이트
pub trait AsyncPartialValidatable<T> {
    type Error;
    
    /// 비동기 방식으로 특정 필드만 검증
    fn validate_field_async<F>(&self, field: F) -> impl Future<Output = Result<(), Self::Error>> + Send
    where
        F: AsRef<str> + Send;
    
    /// 비동기 방식으로 지정된 필드들만 검증
    fn validate_fields_async<I, F>(&self, fields: I) -> impl Future<Output = Result<(), Self::Error>> + Send
    where
        Self: Sync,
        I: IntoIterator<Item = F>,
        F: AsRef<str> + Send,
    {
        // 모든 필드를 벡터로 먼저 수집
        let collected_fields: Vec<F> = fields.into_iter().collect();
        
        async move {
            for field in collected_fields {
                self.validate_field_async(field).await?;
            }
            Ok(())
        }
    }
}

/// 동기 부분 검증 가능한 타입을 비동기 부분 검증으로 확장
impl<T, V> AsyncPartialValidatable<T> for V 
where 
    V: PartialValidatable<T> + Send + Sync,
    V::Error: Send,
{
    type Error = V::Error;
    
    fn validate_field_async<F>(&self, field: F) -> impl Future<Output = Result<(), Self::Error>> + Send
    where
        F: AsRef<str> + Send,
    {
        async move {
            self.validate_field(field)
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

/// 연속 검증 실행을 위한 유틸리티 트레이트
pub trait ValidationChain<T, E> {
    /// 검증 성공 시 다음 검증 단계 실행
    fn and_then<U, F>(self, f: F) -> Result<U, E>
    where
        F: FnOnce(T) -> Result<U, E>;
    
    /// 검증 실패 시 대체 값 사용
    fn or_else<F>(self, f: F) -> Result<T, E>
    where
        F: FnOnce(E) -> Result<T, E>;
    
    /// 검증 실패 시 기본값 사용
    fn or_default(self) -> T
    where
        T: Default;
}

/// Result 타입에 대한 ValidationChain 구현
impl<T, E> ValidationChain<T, E> for Result<T, E> {
    fn and_then<U, F>(self, f: F) -> Result<U, E>
    where
        F: FnOnce(T) -> Result<U, E>,
    {
        self.and_then(f)
    }
    
    fn or_else<F>(self, f: F) -> Result<T, E>
    where
        F: FnOnce(E) -> Result<T, E>,
    {
        self.or_else(f)
    }
    
    fn or_default(self) -> T
    where
        T: Default,
    {
        self.unwrap_or_else(|_| T::default())
    }
}

/// 비동기 연속 검증을 위한 트레이트
pub trait AsyncValidationChain<T, E> {
    /// 비동기 검증 성공 시 다음 검증 단계 실행
    fn and_then_async<U, F, Fut>(self, f: F) -> impl Future<Output = Result<U, E>> + Send
    where
        Self: Send,
        T: Send,
        E: Send,
        F: FnOnce(T) -> Fut + Send,
        Fut: Future<Output = Result<U, E>> + Send;
    
    /// 비동기 검증 실패 시 대체 값 사용
    fn or_else_async<F, Fut>(self, f: F) -> impl Future<Output = Result<T, E>> + Send
    where
        Self: Send,
        T: Send,
        E: Send,
        F: FnOnce(E) -> Fut + Send,
        Fut: Future<Output = Result<T, E>> + Send;
}

impl<T: Send, E: Send> AsyncValidationChain<T, E> for Result<T, E> {
    fn and_then_async<U, F, Fut>(self, f: F) -> impl Future<Output = Result<U, E>> + Send
    where
        F: FnOnce(T) -> Fut + Send,
        Fut: Future<Output = Result<U, E>> + Send,
    {
        async move {
            match self {
                Ok(t) => f(t).await,
                Err(e) => Err(e),
            }
        }
    }
    
    fn or_else_async<F, Fut>(self, f: F) -> impl Future<Output = Result<T, E>> + Send
    where
        F: FnOnce(E) -> Fut + Send,
        Fut: Future<Output = Result<T, E>> + Send,
    {
        async move {
            match self {
                Ok(t) => Ok(t),
                Err(e) => f(e).await,
            }
        }
    }
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