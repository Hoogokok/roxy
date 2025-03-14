use super::typestate::{Raw, TypeState, Validatable, Validated};
use super::error::SettingsError;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

// 로드밸런서 전략 상태를 나타내는 트레이트
pub trait LoadBalancerStrategyState: TypeState + Clone + Send + Sync + 'static {}

// 세 가지 상태 정의: 없음, 라운드 로빈, 가중치 기반
#[derive(Debug, Clone)]
pub struct NoLoadBalancing;
impl TypeState for NoLoadBalancing {}
impl LoadBalancerStrategyState for NoLoadBalancing {}

#[derive(Debug, Clone)]
pub struct RoundRobinStrategy;
impl TypeState for RoundRobinStrategy {}
impl LoadBalancerStrategyState for RoundRobinStrategy {}

#[derive(Debug, Clone)]
pub struct WeightedStrategy;
impl TypeState for WeightedStrategy {}
impl LoadBalancerStrategyState for WeightedStrategy {}

// 로드밸런서 설정 구조체 - 전략 상태가 타입 매개변수로 포함됨
#[derive(Debug, Clone, Deserialize)]
pub struct LoadBalancerSettings<S: TypeState = Raw, LBStrategy: LoadBalancerStrategyState = NoLoadBalancing> {
    #[serde(skip)]
    _marker: PhantomData<(S, LBStrategy)>,
    
    // 각 전략 상태에 따라 특정 필드들이 의미를 가짐
    #[serde(skip_serializing_if = "Option::is_none")]
    weight: Option<usize>,
}

// Raw 상태의 NoLoadBalancing - 구성과 검증만 가능
impl LoadBalancerSettings<Raw, NoLoadBalancing> {
    pub fn new() -> Self {
        Self { _marker: PhantomData, weight: None }
    }
    
    // 구성 메서드
    pub fn with_round_robin(&self) -> LoadBalancerSettings<Raw, RoundRobinStrategy> {
        LoadBalancerSettings::<Raw, RoundRobinStrategy>::new()
    }
    
    pub fn with_weighted(&self, weight: usize) -> Result<LoadBalancerSettings<Raw, WeightedStrategy>, &'static str> {
        // 기본 유효성 검사 (완전한 검증은 validate()에서)
        if weight == 0 {
            return Err("가중치는 0보다 커야 합니다");
        }
        Ok(LoadBalancerSettings::<Raw, WeightedStrategy>::new(weight))
    }
}

// Validated 상태의 NoLoadBalancing - 실제 기능 사용 가능
impl LoadBalancerSettings<Validated, NoLoadBalancing> {
    pub fn new() -> Self {
        Self { _marker: PhantomData, weight: None }
    }
    
    pub fn has_strategy(&self) -> bool {
        false
    }
}

// Raw 상태의 RoundRobin - 구성과 검증만 가능
impl LoadBalancerSettings<Raw, RoundRobinStrategy> {
    pub fn new() -> Self {
        Self { _marker: PhantomData, weight: None }
    }
}

// Validated 상태의 RoundRobin - 실제 기능 사용 가능
impl LoadBalancerSettings<Validated, RoundRobinStrategy> {
    pub fn new() -> Self {
        Self { _marker: PhantomData, weight: None }
    }
    
    pub fn strategy_name(&self) -> &'static str {
        "roundrobin"
    }
    
    pub fn has_strategy(&self) -> bool {
        true
    }
}

// Raw 상태의 Weighted - 구성과 검증만 가능
impl LoadBalancerSettings<Raw, WeightedStrategy> {
    pub fn new(weight: usize) -> Self {
        Self { _marker: PhantomData, weight: Some(weight) }
    }
}

// Validated 상태의 Weighted - 실제 기능 사용 가능
impl LoadBalancerSettings<Validated, WeightedStrategy> {
    pub fn new(weight: usize) -> Self {
        Self { _marker: PhantomData, weight: Some(weight) }
    }
    
    pub fn strategy_name(&self) -> &'static str {
        "weighted"
    }
    
    pub fn weight(&self) -> usize {
        self.weight.unwrap_or(1)
    }
    
    pub fn has_strategy(&self) -> bool {
        true
    }
}

// Raw에서 Validated로 변환
impl Validatable<LoadBalancerSettings<Validated, NoLoadBalancing>> 
    for LoadBalancerSettings<Raw, NoLoadBalancing> 
{
    type Error = SettingsError;
    
    fn validate(self) -> Result<LoadBalancerSettings<Validated, NoLoadBalancing>, Self::Error> {
        Ok(LoadBalancerSettings::<Validated, NoLoadBalancing>::new())
    }
}

impl Validatable<LoadBalancerSettings<Validated, RoundRobinStrategy>> 
    for LoadBalancerSettings<Raw, RoundRobinStrategy> 
{
    type Error = SettingsError;
    
    fn validate(self) -> Result<LoadBalancerSettings<Validated, RoundRobinStrategy>, Self::Error> {
        Ok(LoadBalancerSettings::<Validated, RoundRobinStrategy>::new())
    }
}

impl Validatable<LoadBalancerSettings<Validated, WeightedStrategy>> 
    for LoadBalancerSettings<Raw, WeightedStrategy> 
{
    type Error = SettingsError;
    
    fn validate(self) -> Result<LoadBalancerSettings<Validated, WeightedStrategy>, Self::Error> {
        // 가중치 검증
        if self.weight.unwrap_or(0) == 0 {
            return Err(SettingsError::InvalidValue {
                field: "load_balancer.weight".to_string(),
                context: "로드밸런서 설정".to_string(),
                message: "가중치 기반 로드밸런싱은 0이 아닌 가중치가 필요합니다".to_string(),
            });
        }
        
        Ok(LoadBalancerSettings::<Validated, WeightedStrategy>::new(self.weight.unwrap()))
    }
}

// Deserialize 지원을 위한 구현 (문자열에서 적절한 타입으로 변환)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LoadBalancerStrategyName {
    RoundRobin,
    Weighted,
}

// 기존에 문자열 기반 처리를 위한 변환 유틸리티 기능
impl LoadBalancerStrategyName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RoundRobin => "roundrobin",
            Self::Weighted => "weighted",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "roundrobin" => Some(Self::RoundRobin),
            "weighted" => Some(Self::Weighted),
            _ => None,
        }
    }
}

// 테스트 코드
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_loadbalancer_strategy_types() {
        // Raw 상태에서 구성
        let raw_no_lb = LoadBalancerSettings::<Raw, NoLoadBalancing>::new();
        let raw_round_robin = raw_no_lb.with_round_robin();
        let raw_weighted = raw_no_lb.with_weighted(5).unwrap();
        
        // 검증 후 Validated 상태에서 기능 사용
        let validated_no_lb = raw_no_lb.validate().unwrap();
        assert!(!validated_no_lb.has_strategy());
        
        let validated_round_robin = raw_round_robin.validate().unwrap();
        assert!(validated_round_robin.has_strategy());
        assert_eq!(validated_round_robin.strategy_name(), "roundrobin");
        
        let validated_weighted = raw_weighted.validate().unwrap();
        assert!(validated_weighted.has_strategy());
        assert_eq!(validated_weighted.strategy_name(), "weighted");
        assert_eq!(validated_weighted.weight(), 5);
    }
    
    #[test]
    fn test_loadbalancer_validation() {
        // 전략 없음 검증
        let no_lb = LoadBalancerSettings::<Raw, NoLoadBalancing>::new();
        let validated_no_lb = no_lb.validate();
        assert!(validated_no_lb.is_ok());
        
        // 라운드 로빈 검증
        let round_robin = LoadBalancerSettings::<Raw, RoundRobinStrategy>::new();
        let validated_round_robin = round_robin.validate();
        assert!(validated_round_robin.is_ok());
        
        // 유효한 가중치 기반 검증
        let weighted = LoadBalancerSettings::<Raw, WeightedStrategy>::new(5);
        let validated_weighted = weighted.validate();
        assert!(validated_weighted.is_ok());
        assert_eq!(validated_weighted.unwrap().weight(), 5);
        
        // 잘못된 가중치 검증
        let invalid_weighted = LoadBalancerSettings::<Raw, WeightedStrategy> {
            _marker: PhantomData,
            weight: Some(0),
        };
        let validated_invalid = invalid_weighted.validate();
        assert!(validated_invalid.is_err());
    }
}
