use derive_builder::Builder;

use sim::service_client::ServiceClient;
use sim::worker_server::WorkerServer;
use sim::{worker_server::Worker, ClientRequest, ClientResponse};
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use std::net::SocketAddr;

use clap::Parser;

pub mod sim {
    tonic::include_proto!("sim");
}

#[derive(Debug, Parser)]
struct Opt {
    #[clap(long)]
    worker_addr: SocketAddr
}

#[derive(Debug, Builder)]
pub struct WorkloadWorker {}

impl WorkloadWorker {
    pub async fn send_client_request(target: SocketAddr, request: ClientRequest) -> anyhow::Result<()> {
        let mut client = ServiceClient::connect(format!("http://{}", target)).await?;
        client.handle_client_request(Request::new(request)).await?;

        Ok(())
    }
}

#[tonic::async_trait]
impl Worker for WorkloadWorker {
    async fn handle_client_reply(
        &self,
        response: Request<ClientResponse>
    ) -> Result<Response<()>, Status> {
        println!("Worker received client response: {:?}", response.into_inner());
        Ok(Response::new(()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();
    let worker = WorkloadWorkerBuilder::default().build()?;

    Server::builder()
        .add_service(WorkerServer::new(worker))
        .serve(opt.worker_addr).await.expect("Failed to start worker");

    Ok(())
}