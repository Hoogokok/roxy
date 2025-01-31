use std::convert::Infallible;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use tokio::net::TcpListener;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper_util::rt;

async fn handle_request(
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Empty<Bytes>>, Infallible> {
    // 요청 정보 출력
    println!("Received request: {} {}", req.method(), req.uri());
    
    // 임시 응답 반환
    Ok(hyper::Response::new(Empty::new()))
}

#[tokio::main]
async fn main() {
    // TCP 리스너 생성
    let listener = match TcpListener::bind("0.0.0.0:80").await {
        Ok(listener) => {
            println!("Reverse Proxy listening on port 80");
            listener
        }
        Err(e) => {
            eprintln!("Failed to bind to port 80: {}", e);
            return;
        }
    };

    // 연결 수락 루프
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("Accepted connection from: {}", addr);
                
                // 각 연결에 대해 새로운 태스크 생성
                tokio::task::spawn(async move {
                    let io = rt::TokioIo::new(stream);
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(io, service_fn(handle_request))
                        .await 
                    {
                        eprintln!("Error serving connection: {}", err);
                    }
                });
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }
}
