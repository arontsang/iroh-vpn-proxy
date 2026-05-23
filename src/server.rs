pub mod stun;
pub mod support;
pub mod tunnel;
mod web;

use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use quinn::crypto::rustls::QuicServerConfig;
use quinn::{rustls, AsyncUdpSocket, Incoming, Runtime, TokioRuntime};
use crate::stun::StunSocket;
use crate::support::{get_value_from_env, TokioIo};
use crate::tunnel::handle_proxy_request;
use crate::web::handle_web_request;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {

    let socket = run_quic_proxy().await?;
    handle_web_request(socket).await?;

    Ok(())
}

async fn run_quic_proxy() -> Result<Arc<StunSocket>> {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    let cert = cert.der().clone();
    let key_der = signing_key.serialize_der().try_into().unwrap();


    let server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key_der)?;

    let mut server_config =
        quinn::ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(server_crypto)?));
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());

    let endpoint = SocketAddr::from((
        [0,0,0,0],
        get_value_from_env("QUIC_BIND_PORT").unwrap_or(0)
    ));

    let runtime = Arc::new(TokioRuntime);
    let socket = StunSocket::new(endpoint, runtime.clone())?;

    println!("Listening on {}", socket.local_addr()?);

    let socket = Arc::new(socket);
    let endpoint = quinn::Endpoint::new_with_abstract_socket(
        quinn::EndpointConfig::default(),
        Some(server_config),
        socket.clone(),
        runtime.clone()
    )?;

    runtime.spawn(Box::pin(async move {
        while let Some(conn) = endpoint.accept().await {
            tokio::spawn(async move {
                handle_quic_connection(conn).await.ok();
            });
        }
    }));

    Ok(socket)
}

async fn handle_quic_connection(conn: Incoming) -> Result<()> {
    let conn = conn.await?;
    while let Ok((send, recv)) = conn.accept_bi().await {
        let client = tokio::io::join(recv, send);
        let client = TokioIo::new(client);
        _ = tokio::spawn(async move {
            handle_proxy_request(client);
        });
    }


    Ok(())

}