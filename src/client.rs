mod stun;
mod support;
mod web;

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use anyhow::Result;
use quinn::{ClientConfig, Endpoint, TokioRuntime};
use quinn::crypto::rustls::QuicClientConfig;
use quinn::udp::Transmit;
use tokio::net::{TcpListener};
use crate::stun::{AsyncUdpExt, StunSocket};
use crate::support::{get_value_from_env, SkipServerVerification};
use crate::web::StunRequest;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let client = reqwest::Client::new();
    let server_base = std::env::var("SERVER_ADDR");
    let server_base = server_base?;
    let StunRequest { endpoint: server_endpoint } = reqwest::get(&format!("{}/stun/get_endpoint", server_base)).await?
        .json()
        .await?;

    let endpoint = SocketAddr::from(([0,0,0,0], 0));
    let runtime = Arc::new(TokioRuntime);
    let socket = StunSocket::new(endpoint, runtime.clone())?;
    let socket = Arc::new(socket);

    let mut endpoint = Endpoint::new_with_abstract_socket(
        quinn::EndpointConfig::default(),
        None,
        socket.clone(),
        runtime.clone()
    )?;

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let knockers = tokio::spawn(async move {
        loop {
            client.post(&format!("{}/stun/knock", server_base))
                .json(&StunRequest{endpoint: socket.stun_addr().unwrap()})
                .send()
                .await.ok();

            let knock = Transmit {
                destination: server_endpoint,
                ecn: None,
                contents: &0xDEADBEEF_u32.to_be_bytes(),
                segment_size: None,
                src_ip: None,
            };
            socket.wait_and_send(&knock).await.ok();

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    let client_config = ClientConfig::new(Arc::new(QuicClientConfig::try_from(
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth(),
    )?));
    endpoint
        .set_default_client_config(client_config);
    let connection = endpoint
        .connect(server_endpoint, "localhost")?
        .await?;

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
}

