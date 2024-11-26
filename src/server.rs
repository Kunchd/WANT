use dashmap::{DashMap, DashSet};
use serde::{Deserialize, Serialize};
use sim::service_client::ServiceClient;
use tokio::join;
use std::net::SocketAddr;
use std::sync::{
    Arc,
    Mutex,
};

use tonic::{transport::Server, Request, Response, Status};

use sim::service_server::{Service, ServiceServer};
use sim::{ClientResponse, HelloRequest, HelloResponse, P2a, P2b};

use clap::Parser;

pub mod sim {
    tonic::include_proto!("sim");
}

#[derive(Debug, Parser)]
struct Opt {
    #[clap(long)]
    server_addr: SocketAddr,
    
    #[clap(long, value_parser, num_args = 1.., value_delimiter = ' ')]
    follower_addrs: Vec<SocketAddr>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRequest {
    pub key: String,
    pub value: String,
    pub sn: u32
}

impl TryFrom<sim::ClientRequest> for ClientRequest {
    type Error = anyhow::Error;

    fn try_from(value: sim::ClientRequest) -> Result<Self, Self::Error> {
        Ok(Self { key: value.key, value: value.value, sn: value.sn })
    }
}

impl Into<sim::ClientRequest> for ClientRequest {
    fn into(self) -> sim::ClientRequest {
        sim::ClientRequest {
            key: self.key,
            value: self.value,
            sn: self.sn
        }
    }
}

#[derive(Debug, Default)]
pub struct PaxosService {
    application: DashMap<String, String>,
    log: DashMap<u32, ClientRequest>,
    slot_votes: DashMap<u32, DashSet<u32>>,
    slot_in: Arc<Mutex<u32>>,
    slot_out: Arc<Mutex<u32>>,
    server_id: u32,
    follower_addrs: Vec<SocketAddr>
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

    async fn handle_p2a(
        &self,
        request: Request<P2a>
    ) -> Result<Response<P2b>, Status> {
        println!("Got a p2a");

        let p2a = request.into_inner();
        Ok(Response::new(P2b {
            request: p2a.request,
            slot_num: p2a.slot_num,
            server_id: self.server_id
        }))
    }

    async fn handle_p2b(
        &self,
        request: Request<P2b>
    ) -> Result<Response<()>, Status> {
        println!("Got a p2b: {:?}", request);

        // record p2b ack
        let p2b = request.into_inner();
        self.slot_votes.get(&p2b.slot_num).unwrap().insert(p2b.server_id);

        // update slot out until we've reached first non-chosen value
        let mut slot_out = self.slot_out.lock().unwrap();
        while self.slot_votes.get(&slot_out).unwrap().len() > 1 {
            let request = self.log.get(&slot_out).unwrap();
            // execute command
            let mut client_response = ClientResponse {
                result : String::from(""),
                sn: request.sn
            };

            if request.value == "" {
                client_response.result = self.application.get(&request.key).unwrap().clone();
            } else {
                self.application.insert(request.key.clone(), request.value.clone());
                client_response.result = String::from("Put ok");
            }
            // TODO: send response to client

            *slot_out += 1;
        }

        Ok(Response::new(()))
    }

    async fn handle_client_request(
        &self,
        request: Request<sim::ClientRequest>
    ) -> Result<Response<()>, Status> {
        println!("Got a client request: {:?}", request);

        let client_request = ClientRequest::try_from(request.into_inner()).unwrap();

        // lock s_in to update it
        let mut slot_in = self.slot_in.lock().unwrap();
        let s_in = slot_in.clone();
        *slot_in += 1;

        // log client request
        self.log.insert(s_in, client_request.clone());

        // update vote map to non-empty dashset
        // NOTE: since we haven't sent any p2as yet, initializing the set here should be ok
        let set = DashSet::<u32>::new();
        set.insert(self.server_id);
        self.slot_votes.insert(s_in, set);
        // record self as ack by default

        // send out p2as to get slot chosen
        self.follower_addrs.iter().for_each(|&addr| {
            let cr = client_request.clone();
            tokio::spawn(async move {
                let mut client = ServiceClient::connect(format!("http://{}", addr)).await.unwrap();

                let p2a = tonic::Request::new(P2a {
                    request: Some(cr.into()),
                    slot_num: s_in
                });

                client.handle_p2a(p2a).await.unwrap();
            });
        });

        Ok(Response::new(()))

        // if majority is reached, execute on application

        // println!("Got a client request: {:?}", request);
        // let client_request = request.into_inner();
        // let key = client_request.key;
        // let value = client_request.value;

        // let mut client_response = ClientResponse {
        //     result : String::from(""),
        //     sn: client_request.sn
        // };

        // if value == "" {
        //     client_response.result = self.application.get(&key).unwrap().clone();
        // } else {
        //     self.application.insert(key, value);
        //     client_response.result = String::from("Put ok");
        // }

        // Ok(Response::new(client_response))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();
    let leader = PaxosService { 
        application: DashMap::new(), 
        log: DashMap::new(), 
        slot_votes: DashMap::new(), 
        slot_in: Arc::from(Mutex::new(0)),
        slot_out: Arc::from(Mutex::new(0)), 
        server_id: 0, 
        follower_addrs: opt.follower_addrs.clone()
    };

    let follower1 = PaxosService { 
        application: DashMap::new(), 
        log: DashMap::new(), 
        slot_votes: DashMap::new(), 
        slot_in: Arc::from(Mutex::new(0)),
        slot_out: Arc::from(Mutex::new(0)), 
        server_id: 1, 
        follower_addrs: Vec::new(),
    };

    let (_, _) = join!(
        Server::builder()
        .add_service(ServiceServer::new(leader))
        .serve(opt.server_addr),

        Server::builder()
        .add_service(ServiceServer::new(follower1))
        .serve(opt.follower_addrs.get(0).unwrap().clone())
    );

    Ok(())
}