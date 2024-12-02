use std::net::SocketAddr;

use sim::service_client::ServiceClient;
use sim::ClientRequest;

use rand::Rng;
use tonic::Request;

use clap::Parser;

#[derive(Debug, Parser)]
struct Opt {
    #[clap(long, value_parser, num_args = 1.., value_delimiter = ' ')]
    server_addrs: Vec<SocketAddr>,

    #[clap(long)]
    worker_addr: SocketAddr,

    #[clap(long, short)]
    messages: u32,

    #[clap(long, short)]
    num_keys: u32
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

    let clients_len = clients.len();

    // For each message, generate a random key and send a put request to a random server
    for i in 1..=opt.messages {
        let key = rand::thread_rng().gen_range(0..opt.num_keys);
        let value = rand::thread_rng().gen_range(1e7..1e10);
        let server = rand::thread_rng().gen_range(0..clients_len);
        let put_request = Request::new(ClientRequest {
            key: key.to_string(),
            value: value.to_string(),
            sn: i,
            worker_addr: opt.worker_addr.to_string(),
            time_sent: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as u64,
        });
        clients[server].handle_client_request(put_request).await?;
    }
    
    Ok(())
}