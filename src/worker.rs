use dashmap::DashMap;
use derive_builder::Builder;
use tokio::io::AsyncWriteExt;

use std::sync::Arc;
use sim::service_client::ServiceClient;
use sim::worker_server::WorkerServer;
use sim::{worker_server::Worker, ClientRequest, ClientResponse};
use tokio::sync::Mutex;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tokio::fs::{File, OpenOptions};
use std::net::SocketAddr;

use clap::Parser;

pub mod sim {
    tonic::include_proto!("sim");
}

#[derive(Debug, Parser)]
struct Opt {
    #[clap(long)]
    worker_addr: SocketAddr,

    #[clap(long)]
    out: String
}

#[derive(Debug, Builder)]
pub struct WorkloadWorker {
    #[builder(default)]
    // Map from response to time (ms) received
    responses: DashMap<String, u64>,

    out_file: Arc<Mutex<File>>
}

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
        let client_response = response.into_inner();
        println!("Worker received client response: {:?}", client_response);

        // // Record the time the response was received
        // let time_received = std::time::SystemTime::now()
        //     .duration_since(std::time::UNIX_EPOCH)
        //     .expect("Time went backwards")
        //     .as_millis() as u64;

        // let key = format!("{:?}", client_response);
        // if !self.responses.contains_key(&key) {
        //     self.responses.insert(key, time_received - client_response.time_sent);   
        // }

        // // append response to file
        // let mut file = self.out_file.lock().await;
        // let response_str = format!("{:?},{}\n", client_response, time_received);
        // file.write_all(response_str.as_bytes()).await.expect("Failed to write to file");

        Ok(Response::new(()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();

    let out_file = Arc::new(Mutex::new(
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(opt.out)
                .await
                .expect("Failed to open file"),
        ));
    println!("File opened");

    let worker = WorkloadWorkerBuilder::default()
        .out_file(out_file)
        .build()?;

    Server::builder()
        .add_service(WorkerServer::new(worker))
        .serve(opt.worker_addr).await.expect("Failed to start worker");

    Ok(())
}