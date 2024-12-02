use std::net::SocketAddr;

use sim::service_client::ServiceClient;
use sim::ClientRequest;

use tonic::Request;

use clap::Parser;

#[derive(Debug, Parser)]
struct Opt {
    #[clap(long, value_parser, num_args = 1.., value_delimiter = ' ')]
    server_addrs: Vec<SocketAddr>,

    #[clap(long)]
    worker_addr: SocketAddr
}

pub mod sim {
    tonic::include_proto!("sim");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();
    // connect to each server
    let mut clients = Vec::new();
    for server_addr in opt.server_addrs {
        clients.push(ServiceClient::connect(format!("http://{}", server_addr)).await?);
    }

    // send a put and get request to each server
    let clients_len = clients.len();
    for (i, client) in clients.iter_mut().enumerate() {
        let put_request = Request::new(ClientRequest {
            key: String::from("a"),
            value: String::from("x"),
            sn: i as u32,
            worker_addr: opt.worker_addr.to_string()
        });
        client.handle_client_request(put_request).await?;

        let get_request = Request::new(ClientRequest {
            key: String::from("a"),
            value: String::from(""),
            sn: (i + clients_len) as u32, 
            worker_addr: opt.worker_addr.to_string()
        });
        client.handle_client_request(get_request).await?;
    }
    
    Ok(())
}