# Rust Reverse Proxy with Docker Integration

Docker 컨테이너를 위한 동적 리버스 프록시 서버입니다. 호스트 및 경로 기반 라우팅을 지원하며, Docker 이벤트를 모니터링하여 백엔드 서비스를 자동으로 관리합니다.

## 주요 기능

### 호스트 및 경로 기반 라우팅
- HTTP Host 헤더를 기반으로 요청을 적절한 백엔드 서비스로 라우팅
- 다양한 경로 매칭 방식 지원:
  - 정확한 경로 매칭 (예: `/api`)
  - 프리픽스 매칭 (예: `/api/*`)
  - 정규식 매칭 (예: `^/api/v[0-9]+/.*`)
- 동일한 호스트에 대해 여러 백엔드 서버 지원 (라운드 로빈 방식)
- HTTP 및 HTTPS 프로토콜 지원

### 동적 백엔드 서비스 관리
- Docker 이벤트 실시간 모니터링
- 컨테이너 시작/중지/업데이트에 따른 자동 라우팅 설정
- 라우팅 테이블 실시간 업데이트
- 재시도 메커니즘으로 일시적인 오류 처리

## 설정

### TOML 설정 파일

`PROXY_CONFIG_FILE` 환경 변수로 TOML 설정 파일 경로를 지정할 수 있습니다:

```toml
docker_network = "reverse-proxy-network"
label_prefix = "reverse-proxy."
http_port = 80
https_enabled = false
https_port = 443

[retry]
max_attempts = 3     # 최대 재시도 횟수
interval = 1         # 재시도 간격 (초)

[logging]
format = "text"  # "text" 또는 "json"
level = "info"   # "error", "warn", "info", "debug", "trace"
output = "stdout"  # "stdout" 또는 파일 경로 (예: "proxy.log")
```

### 환경 변수 설정

환경 변수는 TOML 설정을 덮어쓸 수 있습니다.

| 환경 변수 | 설명 | 기본값 |
|-----------|------|--------|
| `PROXY_CONFIG_FILE` | TOML 설정 파일 경로 | - |
| `LOG_FORMAT` | 로그 출력 포맷 (text/json) | `text` |
| `LOG_LEVEL` | 로그 레벨 (error/warn/info/debug/trace) | `info` |
| `LOG_OUTPUT` | 로그 출력 대상 (stdout 또는 파일 경로) | `stdout` |
| `PROXY_DOCKER_NETWORK` | 프록시가 모니터링할 Docker 네트워크 이름 | `proxy` |
| `PROXY_LABEL_PREFIX` | 컨테이너 라벨 접두사 | `reverse-proxy.` |
| `HTTP_PORT` | HTTP 리스너 포트 | `8080` |
| `HTTPS_ENABLED` | HTTPS 활성화 여부 | `false` |
| `HTTPS_PORT` | HTTPS 리스너 포트 | `443` |
| `TLS_CERT_PATH` | TLS 인증서 파일 경로 (HTTPS 활성화 시 필수) | - |
| `TLS_KEY_PATH` | TLS 개인키 파일 경로 (HTTPS 활성화 시 필수) | - |

## 컨테이너 라벨 설정

백엔드 서비스 컨테이너에는 다음 라벨을 설정해야 합니다:

- `reverse-proxy.host`: 서비스의 호스트 이름 (필수)
- `reverse-proxy.port`: 서비스의 포트 번호 (선택, 기본값: 80)
- `reverse-proxy.path`: 서비스의 경로 패턴 (선택, 기본값: "/")
  - 정확한 경로: `/api`
  - 프리픽스: `/api/*`
  - 정규식: `^/api/v[0-9]+/.*`

예시:
```yaml
labels:
  reverse-proxy.host: "api.example.com"
  reverse-proxy.port: "3000"
  reverse-proxy.path: "/api/*"  # /api로 시작하는 모든 요청을 이 서비스로 라우팅
```

## 실행 방법

### Docker Compose 사용

1. `docker-compose.yml` 파일 생성:

```yaml
version: '3'

services:
  reverse-proxy:
    build: .
    ports:
      - "80:80"
      - "443:443"  # HTTPS 사용 시
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - ./certs:/certs  # TLS 인증서 디렉토리
      - ./config:/config  # TOML 설정 파일 디렉토리
    environment:
      - PROXY_DOCKER_NETWORK=proxy
      - HTTP_PORT=80
      - HTTPS_ENABLED=true  # HTTPS 활성화
      - HTTPS_PORT=443
      - TLS_CERT_PATH=/certs/cert.pem
      - TLS_KEY_PATH=/certs/key.pem
    networks:
      - proxy

networks:
  proxy:
    name: proxy
```

2. 프록시 서버 실행:
```bash
docker-compose up -d
```

3. 백엔드 서비스 예시:
```yaml
version: '3'

services:
  api:
    image: nginx
    labels:
      - "reverse-proxy.host=api.example.com"
      - "reverse-proxy.port=80"
    networks:
      - proxy

networks:
  proxy:
    external: true
    name: proxy
```

## 로깅

### 로그 포맷
- Text 포맷: 사람이 읽기 쉬운 형태의 로그
- JSON 포맷: 구조화된 데이터로 로그 수집/분석에 용이

### 로그 레벨
- ERROR: 심각한 오류 (서비스 중단 가능성)
- WARN: 경고 (서비스는 계속되나 주의 필요)
- INFO: 일반 정보 (서버 시작/중지, 요청 처리 등)
- DEBUG: 디버깅 정보
- TRACE: 상세 추적 정보

### 로그 출력
- stdout: 표준 출력으로 로그 전송
- 파일: 지정된 파일로 로그 저장 (자동으로 logs 디렉토리 생성)

### 로그 항목
- 요청/응답 정보 (ID, 메서드, 경로, 상태 코드, 처리 시간)
- 라우팅 결정 및 백엔드 서비스 정보
- Docker 이벤트 (컨테이너 시작/중지/업데이트)
- TLS 핸드쉐이크 및 HTTPS 연결
- 에러 및 경고 메시지

## 라이선스

MIT License

## 미들웨어 프레임워크

HTTP 요청/응답을 처리하는 미들웨어 체인을 지원합니다.

### 미들웨어 설정

미들웨어는 Docker 라벨이나 TOML 파일을 통해 설정할 수 있습니다.

#### Docker 라벨 설정
```
# 기본 설정
rproxy.http.middlewares.cors.type=cors
rproxy.http.middlewares.cors.enabled=true
rproxy.http.middlewares.cors.order=1

# CORS 설정
rproxy.http.middlewares.cors.headers.access-control-allow-origin=*
rproxy.http.middlewares.cors.headers.access-control-allow-methods=GET,POST,PUT,DELETE
```

#### TOML 설정
```toml
[middlewares.cors]
middleware_type = "cors"
enabled = true
order = 1

[middlewares.cors.settings]
"headers.access-control-allow-origin" = "*"
"headers.access-control-allow-methods" = "GET,POST,PUT,DELETE"
```

### 미들웨어 구현

커스텀 미들웨어 구현 예시:
```rust
use async_trait::async_trait;
use crate::middleware::{Middleware, Request, Response, MiddlewareError};

pub struct MyMiddleware {
    name: String,
}

#[async_trait]
impl Middleware for MyMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    async fn handle_request(&self, req: Request) -> Result<Request, MiddlewareError> {
        // 요청 처리 로직
        Ok(req)
    }

    async fn handle_response(&self, res: Response) -> Result<Response, MiddlewareError> {
        // 응답 처리 로직
        Ok(res)
    }
}
```

# Basic 인증 미들웨어

HTTP Basic 인증을 제공하는 미들웨어입니다.

## 기능
- Basic 인증 프로토콜 지원
- bcrypt 해시 알고리즘 ($2a$, $2b$, $2y$)
- 다양한 인증 소스 지원

## 인증 소스
### 1. Docker 라벨
직접 사용자와 해시된 비밀번호를 라벨에 지정합니다.
```yaml
labels:
  - "rproxy.http.middlewares.my-auth.type=basic-auth"
  - "rproxy.http.middlewares.my-auth.basicAuth.users=admin:$2y$05$..."
```

### 2. .htpasswd 파일
Apache 스타일의 .htpasswd 파일을 사용합니다.
```yaml
labels:
  - "rproxy.http.middlewares.my-auth.basicAuth.source=htpasswd"
  - "rproxy.http.middlewares.my-auth.basicAuth.htpasswd.path=/etc/nginx/.htpasswd"
```

### 3. 환경 변수
환경 변수에서 사용자 정보를 로드합니다.
```yaml
labels:
  - "rproxy.http.middlewares.my-auth.basicAuth.source=env"
  - "rproxy.http.middlewares.my-auth.basicAuth.env.prefix=BASIC_AUTH_USER_"
```

### 4. Docker Secrets
Docker secrets에서 사용자 정보를 로드합니다.
```yaml
labels:
  - "rproxy.http.middlewares.my-auth.basicAuth.source=docker-secret"
  - "rproxy.http.middlewares.my-auth.basicAuth.secret.path=/run/secrets/basic-auth"
```

## 비밀번호 해시 생성
```bash
# bcrypt 해시 생성
htpasswd -nbB admin "my-password"
```

# Rate Limit 미들웨어

요청 빈도를 제한하는 미들웨어입니다.

## 기능
- 토큰 버킷 알고리즘 기반의 rate limiting
- 초당 평균 요청 수(average) 및 최대 버스트(burst) 설정 지원
- 클라이언트별 제한 적용 (IP 주소 또는 요청 경로 기반)
- 429 Too Many Requests 응답 및 적절한 헤더 제공

## 설정 방법

### Docker 라벨 설정
```yaml
labels:
  # 기본 설정
  - "rproxy.http.middlewares.my-ratelimit.type=ratelimit"
  - "rproxy.http.middlewares.my-ratelimit.enabled=true"
  
  # Rate Limit 설정
  - "rproxy.http.middlewares.my-ratelimit.rateLimit.average=100"  # 초당 평균 요청 수
  - "rproxy.http.middlewares.my-ratelimit.rateLimit.burst=200"    # 최대 버스트 허용량
```

### TOML 설정
```toml
[middlewares.my-ratelimit]
middleware_type = "ratelimit"
enabled = true
order = 1

[middlewares.my-ratelimit.settings]
"rateLimit.average" = "100"
"rateLimit.burst" = "200"
```

## 응답 헤더
Rate limit 상태를 나타내는 헤더가 응답에 포함됩니다:

- `X-RateLimit-Limit`: 초당 허용되는 요청 수
- `X-RateLimit-Burst`: 최대 버스트 허용량
- `Retry-After`: 제한 초과 시 다음 요청까지 대기 시간 (초)

## 예제

### 웹 서비스에 Rate Limit 적용
```yaml
services:
  web:
    image: nginx
    labels:
      - "rproxy.http.middlewares.web-ratelimit.type=ratelimit"
      - "rproxy.http.middlewares.web-ratelimit.enabled=true"
      - "rproxy.http.middlewares.web-ratelimit.rateLimit.average=2"   # 초당 2개 요청
      - "rproxy.http.middlewares.web-ratelimit.rateLimit.burst=4"     # 최대 4개 버스트
      - "rproxy.http.routers.web.middlewares=web-ratelimit"
```

### API 서비스에 Rate Limit 적용
```yaml
services:
  api:
    image: node
    labels:
      - "rproxy.http.middlewares.api-ratelimit.type=ratelimit"
      - "rproxy.http.middlewares.api-ratelimit.enabled=true"
      - "rproxy.http.middlewares.api-ratelimit.rateLimit.average=50"  # 초당 50개 요청
      - "rproxy.http.middlewares.api-ratelimit.rateLimit.burst=100"   # 최대 100개 버스트
      - "rproxy.http.routers.api.middlewares=api-ratelimit"
```

### 재시도 메커니즘

일시적인 오류가 발생했을 때 자동으로 재시도를 수행합니다:

#### 재시도 가능한 오류
- 연결 실패
- 타임아웃
- 일시적인 서비스 불가

#### 재시도 설정
| 설정 | 설명 | 기본값 |
|------|------|--------|
| `max_attempts` | 최대 재시도 횟수 | 3 |
| `interval` | 재시도 간격 (초) | 1 |

#### 동작 방식
1. 작업 실행 시도
2. 실패 시 재시도 가능 여부 확인
3. 재시도 가능한 경우 설정된 간격만큼 대기 후 재시도
4. 최대 시도 횟수 도달 또는 성공할 때까지 반복

