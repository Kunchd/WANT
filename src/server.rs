use dashmap::DashMap;
use std::net::SocketAddr;

use tonic::{transport::Server, Request, Response, Status};

use sim::service_server::{Service, ServiceServer};
use sim::{ClientRequest, ClientResponse, HelloRequest, HelloResponse};

use clap::Parser;

pub mod sim {
    tonic::include_proto!("sim");
}

#[derive(Debug, Parser)]
struct Opt {
    #[clap(long)]
    server_addr: SocketAddr
}

#[derive(Debug, Default)]
pub struct PaxosService {
    application: DashMap<String, String>
}

#[tonic::async_trait]
impl Service for PaxosService {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloResponse>, Status> {
        println!("Got a request: {:?}", request);

        let reply = HelloResponse {
            message: format!("Hello, {}!", request.into_inner().name),
        };

        Ok(Response::new(reply))
    }

    async fn handle_client_request(
        &self,
        request: Request<ClientRequest>
    ) -> Result<Response<ClientResponse>, Status> {
        println!("Got a client request: {:?}", request);
        let client_request = request.into_inner();
        let key = client_request.key;
        let value = client_request.value;

        let mut client_response = ClientResponse {
            result : String::from(""),
            sn: client_request.sn
        };

        if value == "" {
            client_response.result = self.application.get(&key).unwrap().clone();
        } else {
            self.application.insert(key, value);
            client_response.result = String::from("Put ok");
        }

        Ok(Response::new(client_response))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();
    let addr = opt.server_addr;
    let service = PaxosService::default();

    Server::builder()
        .add_service(ServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}