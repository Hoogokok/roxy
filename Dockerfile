# 빌드 스테이지
FROM rust:latest AS builder

WORKDIR /usr/src/app

# 의존성 먼저 빌드 (캐시 활용)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# 소스 코드 복사 및 빌드
COPY . .
RUN cargo build --release

# 실행 스테이지
FROM debian:slim

# 기본 설정 (한 번만 실행됨)
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd -r -s /bin/false proxy && \
    mkdir -p /var/run && \
    chown -R proxy:proxy /var/run

# 빌드된 바이너리만 복사
COPY --from=builder /usr/src/app/target/release/reverse_proxy_traefik /usr/local/bin/

USER proxy

EXPOSE 80

CMD ["reverse_proxy_traefik"] 