use dashmap::{DashMap, DashSet};
use serde::{Deserialize, Serialize};
use sim::service_client::ServiceClient;
use sim::worker_client::WorkerClient;

use tokio::task::JoinSet;
use tokio::sync::Mutex;
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;

use tonic::{transport::Server, Request, Response, Status};

use sim::service_server::{Service, ServiceServer};
use sim::{ClientResponse, P2a, P2b};

use clap::{ArgAction, Parser};

pub mod sim {
    tonic::include_proto!("sim");
}

#[derive(Debug, Parser)]
struct Opt {
    #[clap(long, short, default_value = "false", action = ArgAction::SetTrue)]
    verbose: bool,

    #[clap(long)]
    server_addr: SocketAddr,
    
    #[clap(long, value_parser, num_args = 1.., value_delimiter = ' ')]
    follower_addrs: Vec<SocketAddr>
}

#[derive(Debug, Clone, Serialize, Deserialize, Ord)]
pub struct ClientRequest {
    pub key: String,
    pub value: String,
    pub sn: u32,
    pub worker_addr: SocketAddr
}

impl PartialEq for ClientRequest {
    fn eq(&self, other: &Self) -> bool {
        self.sn == other.sn
    }
}

impl Eq for ClientRequest {}

impl PartialOrd for ClientRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.sn.partial_cmp(&other.sn)
    }
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
    application: Arc<DashMap<String, String>>,
    log: DashMap<u32, Vec<HashMap<String, ClientRequest>>>,
    slot_votes: DashMap<u32, DashSet<u32>>,
    slot_in: Arc<Mutex<u32>>,
    slot_out: Arc<Mutex<u32>>,
    server_id: u32,
    follower_addrs: Vec<SocketAddr>,
    server_addr_map: DashMap<u32, SocketAddr>,

    // logic for tribble
    command_buffer: Arc<Mutex<BTreeMap<u32, ClientRequest>>>,
    buffer_limit: usize,

    // for test
    verbose: bool
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

    async fn handle_p2a(
        &self,
        p2a: Request<P2a>
    ) -> Result<Response<()>, Status> {
        let p2a = p2a.into_inner();
        if self.verbose {
            println!("Server {} Got a p2a for slot {}", self.server_id, p2a.slot_num);
        }

        
        let response = P2b {
            slot_num: p2a.slot_num,
            server_id: self.server_id,
            data: p2a.data
        };

        // The leader id is hardcoded to 0
        PaxosService::send_p2b(self.server_addr_map.get(&0).unwrap().clone(), response).await.expect("Failed to send p2b");

        Ok(Response::new(()))
    }

    // Locking order: slot_in -> slot_out -> slot_votes
    async fn handle_p2b(
        &self,
        p2b: Request<P2b>
    ) -> Result<Response<()>, Status> {
        let p2b = p2b.into_inner();
        if self.verbose {
            println!("Server {} Got a p2b for slot {} from server {}", self.server_id, p2b.slot_num, p2b.server_id);
        }

        // obtain locks in correct ordering
        let slot_in = self.slot_in.lock().await;
        let mut slot_out = self.slot_out.lock().await;

        // record p2b ack
        self.slot_votes.get(&p2b.slot_num).unwrap().insert(p2b.server_id);

        // update slot out until we've reached first non-chosen value
        while *slot_out < *slot_in && self.slot_votes.get(&slot_out).unwrap().len() >= (self.follower_addrs.len() + 1) / 2 + 1 {
            let trace = self.log.get(&slot_out).unwrap().clone();
            // execute trace in order
            for cut in trace.iter() {
                let mut cur_tasks = JoinSet::new();

                cut.clone().values().for_each(|request| {
                    let request = request.clone();
                    let application_clone = self.application.clone();
                    cur_tasks.spawn(async move {
                        let request = request.clone();
                        let mut client_response = ClientResponse {
                            result : String::from(""),
                            sn: request.sn
                        };

                        if request.value == "" {
                            client_response.result = application_clone.get(&request.key).unwrap().clone();
                        } else {
                            application_clone.insert(request.key.clone(), request.value.clone());
                            client_response.result = String::from("Put ok");
                        }

                        PaxosService::send_client_reply(request.worker_addr, client_response)
                            .await.expect("Failed to send client response");
                    });
                });

                while let Some(res) = cur_tasks.join_next().await {
                    res.expect("Failed to join task");
                }
            }

            *slot_out += 1;
        } 

        Ok(Response::new(()))
    }

    // Locking order: command_buffer -> slot_in -> log -> slot_votes
    async fn handle_client_request(
        &self,
        request: Request<sim::ClientRequest>
    ) -> Result<Response<()>, Status> {
        let client_request = ClientRequest::try_from(request.into_inner()).unwrap();
        if self.verbose {
            println!("Server {} Got a client request: {:?}", self.server_id, client_request);
        }

        // buffer command
        let mut command_buffer = self.command_buffer.lock().await;
        command_buffer.insert(client_request.sn, client_request.clone());

        // flush buffer if it's full
        if command_buffer.len() >= self.buffer_limit {
            // Create execution trace according the ordering edge concept in tribble
            let mut trace: Vec<HashMap<String, ClientRequest>> = Vec::new();

            command_buffer.values().for_each(|request| {
                let mut i = 0;
                if trace.is_empty() {
                    trace.push(HashMap::new());
                }
                // Inv: trace[i] is not None
                while trace.get(i).unwrap().contains_key(&request.key) {
                    i += 1;
                    if i == trace.len() {
                        trace.push(HashMap::new());
                    }
                }
                // Inv: trace[i] is the first trace that doesn't contain request.key
                trace.get_mut(i).unwrap().insert(request.key.clone(), request.clone());
            });

            let server_id = self.server_id;

            // send p2a to get trace chosen
            let mut slot_in = self.slot_in.lock().await;
            let s_in = slot_in.clone();
            *slot_in += 1;

            self.log.insert(s_in, trace.clone());

            // update vote map to non-empty dashset
            // NOTE: since we haven't sent any p2as yet, initializing the set here should be ok
            let set = DashSet::<u32>::new();
            set.insert(server_id); // leader acks by default
            self.slot_votes.insert(s_in, set);

            // send out p2as to get slot chosen
            self.follower_addrs.iter().for_each(|&addr| {
                let tr = trace.clone();
                tokio::spawn(async move {
                    PaxosService::send_p2a(addr, P2a {
                        slot_num: s_in,
                        server_id: server_id,
                        data: bincode::serialize(&tr).unwrap()
                    }).await.expect("Failed to send p2a");
                });
            });

            // reset buffer
            command_buffer.clear();
        }

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

    let mut servers = Vec::new();

    // leader will have default id of 0
    servers.push(ServiceServer::new(PaxosService { 
        application: Arc::from(DashMap::new()), 
        log: DashMap::new(), 
        slot_votes: DashMap::new(), 
        slot_in: Arc::from(Mutex::new(0)),
        slot_out: Arc::from(Mutex::new(0)), 
        server_id: 0, 
        follower_addrs: opt.follower_addrs.clone(),
        server_addr_map: addr_map.clone(),

        command_buffer: Arc::from(Mutex::new(BTreeMap::new())),
        buffer_limit: 2,
        verbose: opt.verbose
    }));

    opt.follower_addrs.iter().enumerate().for_each(|(i, _)| {
        servers.push(ServiceServer::new(
            PaxosService {
                    application: Arc::from(DashMap::new()), 
                    log: DashMap::new(), 
                    slot_votes: DashMap::new(), 
                    slot_in: Arc::from(Mutex::new(0)),
                    slot_out: Arc::from(Mutex::new(0)), 
                    server_id: (i + 1) as u32, 
                    follower_addrs: Vec::new(),
                    server_addr_map: addr_map.clone(),

                    command_buffer: Arc::from(Mutex::new(BTreeMap::new())),
                    buffer_limit: 0,
                    verbose: opt.verbose
                }
        ));
    });

    let mut server_tasks = JoinSet::new();
    servers.iter().enumerate().for_each(|(id, server)| {
        let addr = addr_map.get(&(id as u32)).unwrap().clone();
        let server = server.clone();
        server_tasks.spawn(async move {
            Server::builder()
            .add_service(server)
            .serve(addr).await.expect("Failed to start server");
        });
    });

    while let Some(res) = server_tasks.join_next().await {
        res.expect("Server failed");
    }

    Ok(())
}