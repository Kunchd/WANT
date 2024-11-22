use std::net::SocketAddr;

use sim::service_client::ServiceClient;
use sim::HelloRequest;

use clap::Parser;

#[derive(Debug, Parser)]
struct Opt {
    #[clap(long)]
    server_addr: SocketAddr
}

pub mod sim {
    tonic::include_proto!("sim");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();
    let mut client = ServiceClient::connect(format!("http://{}", opt.server_addr)).await?;

    let request = tonic::Request::new(HelloRequest {
        name: "Tonic".into(),
    });

    let response = client.say_hello(request).await?;

    println!("RESPONSE={:?}", response);
    Ok(())
}