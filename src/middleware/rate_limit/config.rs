use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Rate Limit 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// 초당 평균 요청 수
    #[serde(default = "default_average")]
    pub average: u32,
    
    /// 버스트 허용량
    #[serde(default = "default_burst")]
    pub burst: u32,
    
    /// 측정 기간
    #[serde(default = "default_period")]
    pub period: Duration,
}

fn default_average() -> u32 {
    100 // 기본값: 초당 100 요청
}

fn default_burst() -> u32 {
    50 // 기본값: 50 요청까지 버스트 허용
}

fn default_period() -> Duration {
    Duration::from_secs(1) // 기본값: 1초
}

impl RateLimitConfig {
    /// Docker 라벨에서 설정을 파싱합니다.
    pub fn from_labels(labels: &HashMap<String, String>) -> Result<Self, String> {
        let mut config = Self {
            average: default_average(),
            burst: default_burst(),
            period: default_period(),
        };

        for (key, value) in labels {
            match key.as_str() {
                "rateLimit.average" => {
                    config.average = value.parse().map_err(|_| "Invalid average value")?;
                }
                "rateLimit.burst" => {
                    config.burst = value.parse().map_err(|_| "Invalid burst value")?;
                }
                _ => continue,
            }
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RateLimitConfig {
            average: default_average(),
            burst: default_burst(),
            period: default_period(),
        };

        assert_eq!(config.average, 100);
        assert_eq!(config.burst, 50);
        assert_eq!(config.period, Duration::from_secs(1));
    }

    #[test]
    fn test_from_labels() {
        let mut labels = HashMap::new();
        labels.insert("rateLimit.average".to_string(), "200".to_string());
        labels.insert("rateLimit.burst".to_string(), "100".to_string());

        let config = RateLimitConfig::from_labels(&labels).unwrap();
        assert_eq!(config.average, 200);
        assert_eq!(config.burst, 100);
    }

    #[test]
    fn test_invalid_labels() {
        let mut labels = HashMap::new();
        labels.insert("rateLimit.average".to_string(), "invalid".to_string());

        assert!(RateLimitConfig::from_labels(&labels).is_err());
    }
}