use dashmap::{DashMap, DashSet};
use serde::{Deserialize, Serialize};
use sim::service_client::ServiceClient;
use sim::worker_client::WorkerClient;

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
    pub sn: u32,
    pub worker_addr: SocketAddr
}

impl TryFrom<sim::ClientRequest> for ClientRequest {
    type Error = anyhow::Error;

    fn try_from(value: sim::ClientRequest) -> Result<Self, Self::Error> {
        Ok(Self { 
            key: value.key, 
            value: value.value, 
            sn: value.sn, 
            worker_addr: value.worker_addr.parse().unwrap()
        })
    }
}

impl Into<sim::ClientRequest> for ClientRequest {
    fn into(self) -> sim::ClientRequest {
        sim::ClientRequest {
            key: self.key,
            value: self.value,
            sn: self.sn,
            worker_addr: self.worker_addr.to_string()
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
    follower_addrs: Vec<SocketAddr>,
    server_addr_map: DashMap<u32, SocketAddr>
}

impl PaxosService {
    pub async fn send_p2a(target: SocketAddr, p2a: P2a) -> anyhow::Result<()> {
        let mut client = ServiceClient::connect(format!("http://{}", target)).await?;
        let p2a = tonic::Request::new(p2a);
        client.handle_p2a(p2a).await?;

        Ok(())
    }

    pub async fn send_p2b(target: SocketAddr, p2b: P2b) -> anyhow::Result<()> {
        let mut client = ServiceClient::connect(format!("http://{}", target)).await?;
        let p2b = tonic::Request::new(p2b);
        client.handle_p2b(p2b).await?;

        Ok(())
    }

    pub async fn send_client_reply(target: SocketAddr, client_response: ClientResponse) -> anyhow::Result<()> {
        let mut client = WorkerClient::connect(format!("http://{}", target)).await?;
        let client_response = tonic::Request::new(client_response);
        client.handle_client_reply(client_response).await?;

        Ok(())
    }
}

#[tonic::async_trait]
impl Service for PaxosService {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloResponse>, Status> {
        println!("Got a Hello request");

        let reply = HelloResponse {
            message: format!("Hello, {}!", request.into_inner().name),
        };

        Ok(Response::new(reply))
    }

    async fn handle_p2a(
        &self,
        request: Request<P2a>
    ) -> Result<Response<()>, Status> {
        let p2a = request.into_inner();
        println!("Server: {} Got a p2a: {:?}", self.server_id, p2a.clone());

        
        let response = P2b {
            slot_num: p2a.slot_num,
            server_id: self.server_id
        };

        // The leader id is hardcoded to 0
        PaxosService::send_p2b(self.server_addr_map.get(&0).unwrap().clone(), response).await.expect("Failed to send p2b");

        Ok(Response::new(()))
    }

    async fn handle_p2b(
        &self,
        request: Request<P2b>
    ) -> Result<Response<()>, Status> {
        let p2b = request.into_inner();
        println!("Server {} Got a p2b: {:?}", self.server_id, p2b.clone());

        // record p2b ack
        self.slot_votes.get(&p2b.slot_num).unwrap().insert(p2b.server_id);

        // update slot out until we've reached first non-chosen value
        let mut slot_out = self.slot_out.lock().unwrap();
        let slot_in = self.slot_in.lock().unwrap();
        while *slot_out < *slot_in && self.slot_votes.get(&slot_out).unwrap().len() > 1 {
            let request = self.log.get(&slot_out).unwrap().clone();
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

            tokio::spawn(async move {
                PaxosService::send_client_reply(request.worker_addr, client_response)
                    .await.expect("Failed to send client response");
            });

            *slot_out += 1;
        }

        Ok(Response::new(()))
    }

    async fn handle_client_request(
        &self,
        request: Request<sim::ClientRequest>
    ) -> Result<Response<()>, Status> {
        let client_request = ClientRequest::try_from(request.into_inner()).unwrap();
        println!("Got a client request: {:?}", client_request);

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
        println!("Server {} inserting slot {}", self.server_id, s_in);
        self.slot_votes.insert(s_in, set);
        // record self as ack by default

        // send out p2as to get slot chosen
        self.follower_addrs.iter().for_each(|&addr| {
            tokio::spawn(async move {
                PaxosService::send_p2a(addr, P2a {
                    slot_num: s_in
                }).await.expect("Failed to send p2a");
            });
        });

        Ok(Response::new(()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();

    // construct addr map
    let addr_map = DashMap::new();
    addr_map.insert(0, opt.server_addr);
    opt.follower_addrs.iter().enumerate().for_each(|(i, &addr)| {
        addr_map.insert((i + 1) as u32, addr);
    });

    // Leader will have default if of 0
    let leader = PaxosService { 
        application: DashMap::new(), 
        log: DashMap::new(), 
        slot_votes: DashMap::new(), 
        slot_in: Arc::from(Mutex::new(0)),
        slot_out: Arc::from(Mutex::new(0)), 
        server_id: 0, 
        follower_addrs: opt.follower_addrs.clone(),
        server_addr_map: addr_map.clone()
    };

    let follower1 = PaxosService { 
        application: DashMap::new(), 
        log: DashMap::new(), 
        slot_votes: DashMap::new(), 
        slot_in: Arc::from(Mutex::new(0)),
        slot_out: Arc::from(Mutex::new(0)), 
        server_id: 1, 
        follower_addrs: Vec::new(),
        server_addr_map: addr_map.clone()
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