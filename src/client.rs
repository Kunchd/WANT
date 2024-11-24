use std::net::SocketAddr;

use sim::service_client::ServiceClient;
use sim::{ClientRequest, HelloRequest};

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

    let hello_request = tonic::Request::new(HelloRequest {
        name: "Tonic".into(),
    });

    let response = client.say_hello(hello_request).await?;
    println!("RESPONSE={:?}", response);

    let put_request = tonic::Request::new(ClientRequest {
        key: String::from("a"),
        value: String::from("x"),
        sn: 1
    });

    let response = client.handle_client_request(put_request).await?;
    println!("RESPONSE={:?}", response);

    let get_request = tonic::Request::new(ClientRequest {
        key: String::from("a"),
        value: String::from(""),
        sn: 2
    });

    let response = client.handle_client_request(get_request).await?;
    println!("RESPONSE={:?}", response);
    
    Ok(())
}