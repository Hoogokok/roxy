
# 빌드 스테이지
FROM rust:latest AS builder

WORKDIR /usr/src/app

# 소스 코드 복사
COPY . .

# 빌드
RUN cargo build --release

# 실행 스테이지
FROM debian:bookworm-slim

# 시스템 업데이트 및 필요한 패키지 설치
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# proxy 그룹과 사용자를 조건부로 생성합니다.
RUN if ! getent group proxy > /dev/null 2>&1; then \
      groupadd -r proxy; \
    fi && \
    if ! getent passwd proxy > /dev/null 2>&1; then \
      useradd -r -g proxy -s /sbin/nologin -c "Proxy user" proxy; \
    fi

# Docker 소켓 접근을 위한 설정
RUN mkdir -p /var/run/proxy /app/certs /var/run/docker && \
    chown -R proxy:proxy /var/run/proxy /app /var/run/docker

# 실행 파일 복사 및 권한 설정
COPY --from=builder /usr/src/app/target/release/reverse_proxy_traefik /app/
RUN chown proxy:proxy /app/reverse_proxy_traefik && \
    chmod 500 /app/reverse_proxy_traefik

# 작업 디렉토리 설정
WORKDIR /app


# proxy 사용자로 전환
USER proxy

EXPOSE 80 443

CMD ["/app/reverse_proxy_traefik"]