use crate::routing_v2::error::RoutingError;

/// 호스트 정보를 담는 불변 데이터 구조입니다.
/// 
/// # 필드
/// 
/// * `name` - 호스트 이름 (예: "example.com")
/// * `port` - 선택적 포트 번호
/// * `path` - 선택적 경로 정보
#[derive(Clone, Debug)]
pub struct HostInfo {
    pub name: String,
    pub port: Option<u16>,
    pub path: Option<String>,
}

impl HostInfo {
    /// 호스트 헤더 값에서 HostInfo를 생성합니다.
    /// 
    /// # 인자
    /// 
    /// * `host` - 파싱할 호스트 문자열 (예: "example.com" 또는 "example.com:8080")
    /// 
    /// # 반환
    /// 
    /// 성공 시 `HostInfo` 인스턴스를 포함한 `Ok`를 반환하고,
    /// 실패 시 적절한 `RoutingError`를 포함한 `Err`를 반환합니다.
    /// 
    /// # 예제
    /// 
    /// ```
    /// use reverse_proxy_traefik::routing_v2::HostInfo;
    /// 
    /// let host_info = HostInfo::from_header_value("example.com:8080").unwrap();
    /// assert_eq!(host_info.name, "example.com");
    /// assert_eq!(host_info.port, Some(8080));
    /// ```
    pub fn from_header_value(value: &str) -> Result<Self, RoutingError> {
        if value.is_empty() {
            return Err(RoutingError::InvalidHost {
                host: value.to_string(),
                reason: "Invalid format".to_string(),
            });
        }

        // 호스트와 포트 분리
        let parts: Vec<&str> = value.split(':').collect();
        match parts.len() {
            1 => Ok(HostInfo {
                name: value.to_string(),
                port: None,
                path: None,
            }),
            2 => {
                // 호스트 이름이 비어있는지 확인
                if parts[0].is_empty() {
                    return Err(RoutingError::InvalidHost {
                        host: value.to_string(),
                        reason: "Invalid format".to_string(),
                    });
                }

                let port = parts[1].parse::<u16>().map_err(|_| {
                    RoutingError::InvalidPort {
                        port: parts[1].to_string(),
                        reason: "Invalid format".to_string(),
                    }
                })?;

                if port == 0 {
                    return Err(RoutingError::InvalidPort {
                        port: parts[1].to_string(),
                        reason: "Port must be greater than 0".to_string(),
                    });
                }

                Ok(HostInfo {
                    name: parts[0].to_string(),
                    port: Some(port),
                    path: None,
                })
            }
            _ => Err(RoutingError::InvalidHost {
                host: value.to_string(),
                reason: "Invalid format".to_string(),
            }),
        }
    }
} 