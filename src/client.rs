use std::net::SocketAddr;

use sim::service_client::ServiceClient;
use sim::ClientRequest;

use tonic::Request;

use clap::Parser;

#[derive(Debug, Parser)]
struct Opt {
    #[clap(long)]
    server_addr: SocketAddr,

    #[clap(long)]
    worker_addr: SocketAddr
}

pub mod sim {
    tonic::include_proto!("sim");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();
    let mut client = ServiceClient::connect(format!("http://{}", opt.server_addr)).await?;

    let put_request = Request::new(ClientRequest {
        key: String::from("a"),
        value: String::from("x"),
        sn: 1,
        worker_addr: opt.worker_addr.to_string()
    });
    client.handle_client_request(put_request).await?;

    let get_request = Request::new(ClientRequest {
        key: String::from("a"),
        value: String::from(""),
        sn: 2, 
        worker_addr: opt.worker_addr.to_string()
    });
    client.handle_client_request(get_request).await?;
    
    Ok(())
}