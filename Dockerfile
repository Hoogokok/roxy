# 빌드 스테이지
FROM rust:latest AS builder

WORKDIR /usr/src/app

# 소스 코드 복사
COPY . .

# 릴리즈 모드로 빌드
RUN cargo build --release

# 실행 스테이지
FROM debian:bookworm-slim

# 필요한 SSL 인증서와 CA 인증서 설치
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# 빌드된 바이너리 복사
COPY --from=builder /usr/src/app/target/release/reverse_proxy_traefik /usr/local/bin/

# Docker 소켓 마운트를 위한 디렉토리 생성
RUN if ! id "proxy" &>/dev/null; then \
        useradd -r -s /bin/false proxy || true; \
    fi && \
    mkdir -p /var/run && \
    chown -R proxy:proxy /var/run

USER proxy

EXPOSE 80

CMD ["reverse_proxy_traefik"] 