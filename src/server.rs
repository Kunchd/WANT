use tonic::{transport::Server, Request, Response, Status};

use sim::service_server::{Service, ServiceServer};
use sim::{HelloRequest, HelloResponse};

pub mod sim {
    tonic::include_proto!("sim");
}

#[derive(Debug, Default)]
pub struct MyService {}

#[tonic::async_trait]
impl Service for MyService {
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let service = MyService::default();

    Server::builder()
        .add_service(ServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}