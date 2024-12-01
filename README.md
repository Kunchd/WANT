To run server, use `cargo run --bin server -- --server-addr <addr> --follower-addrs <addr addr addr ...>`

TO run worker, use `cargo run --bin worker -- --worker-addr <addr>`

To run client, use `cargo run --bin client -- --server-addr <addr> --worker-addr <addr>`

```
cargo run --bin server -- --server-addr 127.0.0.1:50050 --follower-addrs 127.0.0.1:50051
cargo run --bin worker -- --worker-addr 127.0.0.1:60000
cargo run --bin client -- --server-addr 127.0.0.1:50050 --worker-addr 127.0.0.1:60000
```