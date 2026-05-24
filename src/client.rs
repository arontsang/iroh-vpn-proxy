mod stun;
mod support;
mod web;

use anyhow::Result;
use std::net::{SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use iroh::Endpoint;
use iroh::endpoint::{presets, Connection};
use iroh_tickets::endpoint::EndpointTicket;
use tokio::net::TcpListener;
use crate::support::get_value_from_env;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let server_base = std::env::var("SERVER_ADDR")?;
    let ticket = reqwest::get(&format!("{}/iroh/ticket", server_base)).await?
        .text()
        .await?;

    let endpoint = Endpoint::bind(presets::N0).await?;
    let ticket = EndpointTicket::from_str(&ticket)?;
    let connection = endpoint.connect(ticket, "stun-proxy".as_bytes()).await?;

    loop {

        let socket = SocketAddr::from(([0, 0, 0, 0], get_value_from_env::<u16>("PROXY_PORT").unwrap_or(0)));
        let socket = TcpListener::bind(socket).await?;

        loop {
            let (mut incoming, _) = socket.accept().await?;
            let (send, recv) = connection.open_bi().await?;
            tokio::spawn(async move {

                let mut upstream = tokio::io::join(recv, send);
                tokio::io::copy_bidirectional(&mut incoming, &mut upstream).await.ok();
            });
        }
    }

    Ok(())
}

struct Handler {
    connection: tokio::sync::oneshot::Sender<Connection>,
}

