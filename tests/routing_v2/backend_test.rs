use std::sync::atomic::AtomicUsize;
use reverse_proxy_traefik::routing_v2::{
    BackendService,
    backend::LoadBalancerStrategy,
    error::BackendError,
};

#[test]
fn test_basic_backend_service() {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let service = BackendService::new(addr);
    
    // 기본 상태 확인
    assert_eq!(service.address, addr);
    assert!(service.load_balancer.is_none());
    assert_eq!(service.get_next_address().unwrap(), addr);
}

#[test]
fn test_round_robin_load_balancing() {
    let addr1 = "127.0.0.1:8080".parse().unwrap();
    let addr2 = "127.0.0.1:8081".parse().unwrap();
    let addr3 = "127.0.0.1:8082".parse().unwrap();
    
    let mut service = BackendService::new(addr1);
    
    // 라운드 로빈 로드밸런서 활성화
    service.enable_load_balancer(LoadBalancerStrategy::RoundRobin {
        current_index: AtomicUsize::new(0)
    });
    
    // 추가 주소 등록
    service.add_address(addr2, 1).unwrap();
    service.add_address(addr3, 1).unwrap();
    
    // 순차적 순환 확인
    assert_eq!(service.get_next_address().unwrap(), addr1);
    assert_eq!(service.get_next_address().unwrap(), addr2);
    assert_eq!(service.get_next_address().unwrap(), addr3);
    assert_eq!(service.get_next_address().unwrap(), addr1);  // 다시 처음으로
}

#[test]
fn test_weighted_load_balancing() {
    let addr1 = "127.0.0.1:8080".parse().unwrap();
    let addr2 = "127.0.0.1:8081".parse().unwrap();
    
    let mut service = BackendService::new(addr1);
    
    // 가중치 기반 로드밸런서 활성화
    service.enable_load_balancer(LoadBalancerStrategy::Weighted {
        current_index: AtomicUsize::new(0),
        total_weight: 1,
    });
    
    // addr2를 2배 가중치로 추가
    service.add_address(addr2, 2).unwrap();
    
    // 가중치에 따른 분배 확인 (3회 순환)
    let mut addr1_count = 0;
    let mut addr2_count = 0;
    
    for _ in 0..9 {  // 총 가중치 3의 3배
        let addr = service.get_next_address().unwrap();
        if addr == addr1 {
            addr1_count += 1;
        } else {
            addr2_count += 1;
        }
    }
    
    assert_eq!(addr1_count, 3);  // 가중치 1
    assert_eq!(addr2_count, 6);  // 가중치 2
}

#[test]
fn test_load_balancer_error_cases() {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let mut service = BackendService::new(addr);
    
    // 로드밸런서 비활성화 상태에서 주소 추가 시도
    let result = service.add_address("127.0.0.1:8081".parse().unwrap(), 1);
    assert!(matches!(result, Err(BackendError::LoadBalancerNotEnabled)));
    
    // 로드밸런서 활성화 후 정상 동작 확인
    service.enable_load_balancer(LoadBalancerStrategy::RoundRobin {
        current_index: AtomicUsize::new(0)
    });
    assert!(service.add_address("127.0.0.1:8081".parse().unwrap(), 1).is_ok());
} 