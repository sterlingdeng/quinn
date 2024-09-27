//! This example intends to use the smallest amount of code to make a simple QUIC connection.
//!
//! Checkout the `README.md` for guidance.

use std::error::Error;
use std::time::{Duration, Instant};

mod common;
use common::{make_client_endpoint, make_server_endpoint};

use bytes::BufMut;

use tracing::{self, info, trace, trace_span};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let test_length = Duration::from_secs(15);
    // This should match approximately what the interpacket delay is.
    let send_interval = Duration::from_millis(250);

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let server_addr = "127.0.0.1:20001".parse().unwrap();
    let (endpoint, server_cert) = make_server_endpoint(server_addr)?;
    let endpoint2 = endpoint.clone();
    let handle = tokio::spawn(async move {
        let span = trace_span!("SERVER");
        let _guard = span.enter();
        let incoming_conn = endpoint2.accept().await.unwrap();
        let conn = incoming_conn.await.unwrap();
        trace!("connection accepted: addr={}", conn.remote_address());
        loop {
            match conn.read_datagram().await {
                Ok(v) => {
                    let _ = String::from_utf8(v.to_vec()).unwrap();
                }
                Err(e) => match e {
                    proto::ConnectionError::ConnectionClosed(_)
                    | proto::ConnectionError::ApplicationClosed(_) => {
                        return;
                    }
                    _ => {
                        println!("connection error:{}", e);
                        return;
                    }
                },
            }
        }
    });

    let span = trace_span!("CLIENT");
    let _guard = span.enter();
    let client_endpoint = make_client_endpoint("0.0.0.0:20002".parse().unwrap(), &[&server_cert])?;
    // connect to server
    let connection = client_endpoint
        .connect(server_addr, "localhost")
        .unwrap()
        .await
        .unwrap();
    trace!("connected: addr={}", connection.remote_address());

    let end = Instant::now().checked_add(test_length).unwrap();

    let mut buf = bytes::BytesMut::new();
    buf.put(&b"foobarbaz"[..]);
    let buf = buf.freeze();
    while Instant::now() < end {
        connection.send_datagram(buf.clone()).unwrap();
        tokio::time::sleep(send_interval).await;
    }

    drop(connection);
    drop(_guard);

    handle.await.unwrap();

    info!("test exiting..");
    Ok(())
}
