To run server, use `cargo run --bin paxos-server -- --server-addr <addr> --follower-addrs <addr addr addr ...>`

TO run worker, use `cargo run --bin worker -- --worker-addr <addr>`

To run client, use `cargo run --bin client -- --server-addr <addr> --worker-addr <addr>`

To run Paxos servers
```
cargo run --bin paxos-server -- --server-addr 127.0.0.1:50050 --follower-addrs 127.0.0.1:50051 127.0.0.1:50052
cargo run --bin worker -- --worker-addr 127.0.0.1:60000 --out ./data/test.log
cargo run --bin client -- --server-addrs 127.0.0.1:50050 --worker-addr 127.0.0.1:60000 -m 10 -n 10
```

To run Mencius servers
```
cargo run --bin mencius-server -- --server-addrs 127.0.0.1:50050 127.0.0.1:50051 127.0.0.1:50052
cargo run --bin worker -- --worker-addr 127.0.0.1:60000
cargo run --bin client -- --server-addrs 127.0.0.1:50050 127.0.0.1:50051 127.0.0.1:50052 --worker-addr 127.0.0.1:60000
```