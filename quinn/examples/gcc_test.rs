//! This example intends to use the smallest amount of code to make a simple QUIC connection.
//!
//! Checkout the `README.md` for guidance.

use std::error::Error;
use std::fs::OpenOptions;
use std::time::{Duration, Instant};

mod common;
use common::{make_client_endpoint, make_server_endpoint};

use tracing::{self, error, info, trace, trace_span, Instrument};
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let test_length = Duration::from_secs(90);

    // If static_interval is None, then the interval is paced by the GCC estimate.
    let static_interval = None;

    // This is the sink for the GCC controller visualization output.
    let metrics_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("gcc_output.log")
        .unwrap();

    Registry::default()
        .with(
            fmt::layer()
                .compact()
                .with_ansi(true)
                .with_filter(EnvFilter::from_default_env()),
        )
        .with(
            fmt::layer()
                .json()
                .with_writer(metrics_file)
                .with_filter(EnvFilter::from("quinn_proto::congestion::gcc[stats]=trace")),
        )
        .init();

    let server_addr = "127.0.0.1:7777".parse().unwrap();
    let (server_endpoint, server_cert) = make_server_endpoint(server_addr)?;
    let handle = tokio::spawn(
        async move {
            // Server thread
            let incoming_conn = server_endpoint.accept().await.unwrap();
            let server_conn = incoming_conn.await.unwrap();
            trace!("connection accepted: addr={}", server_conn.remote_address());
            let buf = bytes::BytesMut::zeroed(1000);
            let buf = buf.freeze();
            loop {
                match server_conn.read_datagram().await {
                    Ok(v) => {
                        let _ = String::from_utf8(v.to_vec()).unwrap();
                        server_conn.send_datagram(buf.clone()).unwrap();
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
        }
        .instrument(trace_span!("server")),
    );

    let client_endpoint = make_client_endpoint("0.0.0.0:7778".parse().unwrap(), &[&server_cert])?;
    // connect to server
    let span = trace_span!("client");
    let _guard = span.enter();
    let connection = client_endpoint
        .connect(server_addr, "localhost")
        .unwrap()
        .await
        .unwrap();
    trace!("connected: addr={}", connection.remote_address());

    let end = Instant::now().checked_add(test_length).unwrap();

    let buf = bytes::BytesMut::zeroed(1000);
    let buf = buf.freeze();
    tokio::spawn({
        let connection = connection.clone();
        async move {
            loop {
                match connection.read_datagram().await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("err: {}", e);
                        return;
                    }
                }
            }
        }
    });

    // This is the loop that writes data to the server.
    let mut i = 0;
    while Instant::now() < end {
        connection.send_datagram(buf.clone()).unwrap();

        // GCC bitrate is retrieved here.
        let bitrate = connection
            .congestion_state()
            .into_any()
            .downcast_ref::<proto::congestion::Gcc>()
            .unwrap()
            .target_bitrate();

        // Dynamic interval below acts as a dynamic sender that tries to output a bitrate
        // thats requested by GCC.
        // Higher fidelity can be achieved if from_micros is used.
        let interval = static_interval.unwrap_or(Duration::from_millis(
            (((1000 * 8) as f32 / bitrate as f32) * 1_000.) as u64,
        ));

        if i % 100 == 0 {
            // This is just convenience to print out whats happening in the loop.
            println!("Interval: {}ms, Bitrate: {}", interval.as_millis(), bitrate);
        }

        tokio::time::sleep(interval).await;
        i += 1;
    }

    println!("dropping connection");
    connection.close(0u32.into(), b"");
    drop(connection);

    println!("awaiting handle");
    handle.await.unwrap();

    info!("test exiting..");
    Ok(())
}
