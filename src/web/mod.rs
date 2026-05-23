use crate::stun::AsyncUdpExt;
use crate::stun::StunSocket;
use crate::support::get_value_from_env;
use axum::routing::{get, post};
use axum::{extract::{Query, State}, Json, Router};
use quinn::udp::Transmit;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
struct AppState {
    socket: Arc<StunSocket>
}

pub async fn handle_web_request(socket: Arc<StunSocket>) -> anyhow::Result<()> {
    let state = AppState { socket };

    let app = Router::new()
        .route("/keep_alive", get(keep_alive))
        .route("/stun/get_endpoint", get(get_stun_server_addr))
        .route("/stun/knock", post(do_stun_knock))
        .with_state(state);

    let port = get_value_from_env("HTTP_PORT").unwrap_or(8080u16);
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;

    Ok(axum::serve(listener, app).await?)
}

#[derive(Serialize)]
#[derive(serde::Deserialize)]
pub struct StunRequest {
    pub endpoint: SocketAddr,
}


async fn do_stun_knock(State(state): State<AppState>, Json(stun_request): Json<StunRequest>) -> () {
    println!("Knocking at {:?}", stun_request.endpoint);

    for _ in 0..10 {
        let stun_knock = Transmit {
            destination: stun_request.endpoint,
            ecn: None,
            src_ip: None,
            segment_size: None,
            contents: &0xDEADBEEF_u32.to_be_bytes(),
        };

        state.socket.wait_and_send(&stun_knock).await.ok();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn get_stun_server_addr(State(state): State<AppState>) -> Json<StunRequest> {
    let my_stun_addr = state.socket.stun_addr().unwrap();
    Json(StunRequest{endpoint: my_stun_addr })
}

#[derive(Deserialize)]
struct KeepAliveRequest {
    pub time_out_secs: u16
}

async fn keep_alive(Query(query): Query<KeepAliveRequest>) -> () {
    let timeout = query.time_out_secs as u64;
    tokio::time::sleep(Duration::from_secs(timeout)).await;
}