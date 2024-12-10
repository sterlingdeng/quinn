//! This example intends to use the smallest amount of code to make a simple QUIC connection.
//!
//! Checkout the `README.md` for guidance.

use proto::congestion::GccConfig;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod common;
use common::{make_client_endpoint, make_server_endpoint};

use tracing::{self, error, info, trace, trace_span, Instrument};
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};

use serde_json;

use simulation::{models::Manifest, Simulation};
use ts_core::TrafficShaper;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let test_length = Duration::from_secs(90);
    let send_interval = Duration::from_millis(33);

    let metrics_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("metrics.log")
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
            // SERVER
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
        .instrument(trace_span!("SERVER")),
    );

    /*
    let sim_handle = tokio::spawn(
        async {
            let mut file = OpenOptions::new()
                .read(true)
                .open("./manifest.json")
                .unwrap();

            let mut contents = String::new();
            let _ = file.read_to_string(&mut contents);
            let manifest: Manifest = serde_json::from_str(&contents).unwrap();
            let mut sim = Simulation::new(manifest, Instant::now());
            println!("starting sim");
            match sim.start().await {
                Err(e) => println!("error running simulation: {}", e),
                _ => {}
            }
        }
        .instrument(trace_span!("SIM")),
    );
        */

    let cc = GccConfig::new(true);

    let client_endpoint = make_client_endpoint(
        "0.0.0.0:7778".parse().unwrap(),
        &[&server_cert],
        Arc::new(cc),
    )?;
    // connect to server
    let span = trace_span!("CLIENT");
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
    let connection2 = connection.clone();
    tokio::spawn(
        async move {
            loop {
                match connection2.read_datagram().await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("err: {}", e);
                        return;
                    }
                }
            }
        }
        .instrument(trace_span!("READ_CLIENT")),
    );

    let mut last_interval = Duration::ZERO;
    let mut i = 0;
    while Instant::now() < end {
        connection.send_datagram(buf.clone()).unwrap();

        let bitrate = connection
            .congestion_state()
            .into_any()
            .downcast_ref::<proto::congestion::Gcc>()
            .unwrap()
            .target_bitrate();

        let interval =
            Duration::from_micros((((1000 * 8) as f32 / bitrate as f32) * 1_000_000.) as u64);
        //let interval = Duration::from_millis(10);
        if i % 100 == 0 {
            println!("interval: {}ms, bitrate: {}", interval.as_millis(), bitrate);
        }

        tokio::time::sleep(interval).await;
        i += 1;
        last_interval = interval
    }

    println!("dropping connection");
    connection.close(0u32.into(), b"");
    drop(connection);

    //drop(_guard);

    println!("awaiting handle");
    handle.await.unwrap();

    info!("test exiting..");
    Ok(())
}

struct TrafficShapeManifest {
    config: TrafficShapeConfig,
    events: Vec<TrafficShapeEvent>,
}

struct TrafficShapeConfig {
    Device: String,
    Latency: u64,
    TargetBW: u64,
    PacketLoss: u64,
    Addr: String,
    Proto: String,
    Port: String,
}

struct TrafficShapeEvent {
    T: String,
    PacketLoss: u64,
    Latency: u64,
}

async fn traffic_shape() {}
