use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    match TcpListener::bind("0.0.0.0:80").await {
        Ok(listener) => {
            println!("Reverse Proxy listening on port 80");
        }
        Err(e) => {
            eprintln!("Failed to bind to port 80: {}", e);
        }
    }
}
