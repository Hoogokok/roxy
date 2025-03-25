use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use tokio::runtime::Runtime;
use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;
use futures_util::future::join_all;
use tempfile;

use roxy::settings::container::ContainerConfigManager;
use roxy::settings::json::JsonConfig;

/// 설정 로드 벤치마크
fn bench_load_config(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // 테스트용 임시 디렉토리 및 JSON 파일 생성
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // 유효한 JSON 파일 생성
    let json_content = r#"{"server": {"http": {"port": 8080}}}"#;
    std::fs::write(&config_path, json_content).unwrap();

    let mut group = c.benchmark_group("container_config_load");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("load_single_config", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (manager, _rx) = ContainerConfigManager::new();
                manager.load_container_config("container1".to_string(), &config_path).await.unwrap();
            });
        });
    });

    group.finish();
}

/// 설정 병합 벤치마크
fn bench_merge_config(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // 테스트용 라벨 생성
    let mut labels = HashMap::new();
    labels.insert("roxy.host".to_string(), "example.com".to_string());
    labels.insert("roxy.server.http_port".to_string(), "9090".to_string());
    
    let mut group = c.benchmark_group("container_config_merge");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    
    // 다양한 설정 크기에 대한 벤치마크
    for size in [5, 10, 20].iter() {
        group.bench_with_input(BenchmarkId::new("merge_with_docker_labels", size), size, |b, &size| {
            // 지정된 크기의 설정 생성
            let mut test_labels = labels.clone();
            for i in 0..size {
                test_labels.insert(format!("roxy.test.value.{}", i), format!("test-{}", i));
            }
            
            b.iter(|| {
                rt.block_on(async {
                    let (manager, _rx) = ContainerConfigManager::new();
                    // JSON 설정 등록
                    manager.container_configs.insert("container1".to_string(), JsonConfig::default());
                    // 설정 병합
                    let _settings = manager.merge_config("container1", &test_labels);
                });
            });
        });
    }
    
    group.finish();
}

/// 동시 접근 벤치마크
fn bench_concurrent_access(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // 테스트용 임시 디렉토리 및 JSON 파일 생성
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // 유효한 JSON 파일 생성
    let json_content = r#"{"server": {"http": {"port": 8080}}}"#;
    std::fs::write(&config_path, json_content).unwrap();
    
    let mut group = c.benchmark_group("container_config_concurrent");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    
    // 다양한 동시 접근 수에 대한 벤치마크
    for concurrency in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("concurrent_loads", concurrency), concurrency, |b, &concurrency| {
            b.iter(|| {
                rt.block_on(async {
                    let (manager, _rx) = ContainerConfigManager::new();
                    let manager = Arc::new(manager);
                    
                    let mut tasks = Vec::new();
                    for i in 0..concurrency {
                        let manager_clone = manager.clone();
                        let container_id = format!("container{}", i);
                        let config_path = config_path.clone();
                        
                        tasks.push(tokio::spawn(async move {
                            manager_clone.load_container_config(container_id, &config_path).await.unwrap();
                        }));
                    }
                    
                    join_all(tasks).await;
                });
            });
        });
        
        group.bench_with_input(BenchmarkId::new("concurrent_merges", concurrency), concurrency, |b, &concurrency| {
            b.iter(|| {
                rt.block_on(async {
                    let (manager, _rx) = ContainerConfigManager::new();
                    // 모든 컨테이너에 대한 JSON 설정 미리 등록
                    for i in 0..concurrency {
                        manager.container_configs.insert(format!("container{}", i), JsonConfig::default());
                    }
                    
                    let manager = Arc::new(manager);
                    let mut labels = HashMap::new();
                    labels.insert("roxy.host".to_string(), "example.com".to_string());
                    
                    let mut tasks = Vec::new();
                    for i in 0..concurrency {
                        let manager_clone = manager.clone();
                        let container_id = format!("container{}", i);
                        let labels = labels.clone();
                        
                        tasks.push(tokio::spawn(async move {
                            let _settings = manager_clone.merge_config(&container_id, &labels);
                        }));
                    }
                    
                    join_all(tasks).await;
                });
            });
        });
    }
    
    group.finish();
}

criterion_group!(benches, bench_load_config, bench_merge_config, bench_concurrent_access);
criterion_main!(benches); 