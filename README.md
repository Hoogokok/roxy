# Rust Reverse Proxy with Docker Integration

Docker 컨테이너를 위한 동적 리버스 프록시 서버입니다. 호스트 기반 라우팅을 지원하며, Docker 이벤트를 모니터링하여 백엔드 서비스를 자동으로 관리합니다.

## 주요 기능

### 호스트 기반 라우팅
- HTTP Host 헤더를 기반으로 요청을 적절한 백엔드 서비스로 라우팅
- 동일한 호스트에 대해 여러 백엔드 서버 지원 (라운드 로빈 방식)
- HTTP 및 HTTPS 프로토콜 지원

### 동적 백엔드 서비스 관리
- Docker 이벤트 실시간 모니터링
- 컨테이너 시작/중지/업데이트에 따른 자동 라우팅 설정
- 라우팅 테이블 실시간 업데이트

## 환경 변수 설정

| 환경 변수 | 설명 | 기본값 |
|-----------|------|--------|
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

예시:
```yaml
labels:
  reverse-proxy.host: "api.example.com"
  reverse-proxy.port: "3000"
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

- 구조화된 JSON 로깅 지원
- 요청/응답 정보, 라우팅 결정, 에러 등 상세 로깅
- Docker 이벤트 및 백엔드 서비스 상태 변경 추적
- TLS 핸드쉐이크 및 HTTPS 연결 관련 로그

## 라이선스

MIT License

