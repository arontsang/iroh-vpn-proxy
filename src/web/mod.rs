use crate::support::get_value_from_env;
use axum::routing::get;
use axum::{extract::{Query, State}, Router};
use iroh_tickets::endpoint::EndpointTicket;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
struct AppState {
    router: Arc<iroh::protocol::Router>
}

pub async fn handle_web_request(router: Arc<iroh::protocol::Router>) -> anyhow::Result<()> {
    let state = AppState { router };

    let app = Router::new()
        .route("/keep_alive", get(keep_alive))
        .route("/iroh/ticket", get(get_ticket))
        .with_state(state);

    let port = get_value_from_env("HTTP_PORT").unwrap_or(8080u16);
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;

    Ok(axum::serve(listener, app).await?)
}


async fn get_ticket(State(state): State<AppState>) -> String {
    let endpoint = state.router.endpoint();
    let ticket = EndpointTicket::new(endpoint.addr());
    ticket.to_string()
}

#[derive(Deserialize)]
struct KeepAliveRequest {
    pub time_out_secs: u16
}

async fn keep_alive(Query(query): Query<KeepAliveRequest>) -> () {
    let timeout = query.time_out_secs as u64;
    tokio::time::sleep(Duration::from_secs(timeout)).await;
}