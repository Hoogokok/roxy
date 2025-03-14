use super::typestate::TypeState;

// 1. 로드밸런서 상태 트레이트 정의
pub trait LoadBalancerState: TypeState + Clone + Send + Sync + 'static {}

// 2. 활성화/비활성화 상태 타입 정의
#[derive(Debug, Clone)]
pub struct LoadBalancerEnabled;
impl TypeState for LoadBalancerEnabled {}
impl LoadBalancerState for LoadBalancerEnabled {}

#[derive(Debug, Clone)]
pub struct LoadBalancerDisabled;
impl TypeState for LoadBalancerDisabled {}
impl LoadBalancerState for LoadBalancerDisabled {}

// 3. 기본 테스트
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_loadbalancer_state_traits() {
        // 구체적인 타입으로 테스트
        let _enabled = LoadBalancerEnabled {};
        let _disabled = LoadBalancerDisabled {};
        
        // 타입 상태 패턴은 일반적으로 컴파일 타임 다형성을 사용
        fn _use_enabled<T: LoadBalancerState>(_: T) {}
        _use_enabled(LoadBalancerEnabled {});
        _use_enabled(LoadBalancerDisabled {});
    }
}
