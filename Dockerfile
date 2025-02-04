# 빌드 스테이지
FROM rust:latest AS builder

WORKDIR /usr/src/app

# 소스 코드 복사
COPY . .

# 빌드
RUN cargo build --release

# 실행 스테이지
FROM debian:bookworm-slim  
# 기본 설정
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    mkdir -p /var/run && \
    chown -R proxy:proxy /var/run

USER proxy

# 빌드된 바이너리 복사
COPY --from=builder /usr/src/app/target/release/reverse_proxy_traefik /usr/local/bin/

EXPOSE 80

CMD ["reverse_proxy_traefik"]