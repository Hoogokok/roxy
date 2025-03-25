use std::sync::atomic::AtomicUsize;
use reverse_proxy_traefik::routing_v2::{
    BackendService,
    backend::LoadBalancerStrategy,
    error::BackendError,
};
use std::net::SocketAddr;

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
fn test_round_robin_strategy() {
    // 1. 백엔드 서비스 생성
    let addr1: SocketAddr = "127.0.0.1:8001".parse().unwrap();
    let mut service = BackendService::new(addr1);

    // 2. 추가 백엔드 주소들
    let addr2: SocketAddr = "127.0.0.1:8002".parse().unwrap();
    let addr3: SocketAddr = "127.0.0.1:8003".parse().unwrap();

    // 3. 라운드로빈 전략 활성화
    service.enable_load_balancer(LoadBalancerStrategy::RoundRobin {
        current_index: AtomicUsize::new(0),
    });

    // 4. 추가 주소 등록
    service.add_address(addr2, 1).unwrap();
    service.add_address(addr3, 1).unwrap();

    // 5. 순차적 분배 확인
    assert_eq!(service.get_next_address().unwrap(), addr1);
    assert_eq!(service.get_next_address().unwrap(), addr2);
    assert_eq!(service.get_next_address().unwrap(), addr3);
    assert_eq!(service.get_next_address().unwrap(), addr1); // 다시 처음으로
}

#[test]
fn test_weighted_strategy() {
    // 1. 백엔드 서비스 생성
    let addr1: SocketAddr = "127.0.0.1:8001".parse().unwrap();
    let mut service = BackendService::new(addr1);

    // 2. 추가 백엔드 주소 (가중치 다르게)
    let addr2: SocketAddr = "127.0.0.1:8002".parse().unwrap();

    // 3. 가중치 기반 전략 활성화 (addr1: 1, addr2: 2)
    service.enable_load_balancer(LoadBalancerStrategy::Weighted {
        current_index: AtomicUsize::new(0),
        total_weight: 1,
    });

    // 4. 가중치 2인 주소 추가
    service.add_address(addr2, 2).unwrap();

    // 5. 가중치에 따른 분배 확인 (1:2 비율)
    let mut addr1_count = 0;
    let mut addr2_count = 0;

    for _ in 0..30 {
        match service.get_next_address().unwrap() {
            addr if addr == addr1 => addr1_count += 1,
            addr if addr == addr2 => addr2_count += 1,
            _ => unreachable!(),
        }
    }

    // addr2가 addr1의 약 2배 정도 선택되어야 함
    assert!(addr2_count > addr1_count * 3/2, 
        "addr2 (count: {}) should be selected about twice as much as addr1 (count: {})",
        addr2_count, addr1_count);
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