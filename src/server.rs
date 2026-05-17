pub mod support;
pub mod tunnel;

use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use quinn::crypto::rustls::QuicServerConfig;
use quinn::{rustls, Incoming, Runtime, TokioRuntime};
use crate::support::{get_value_from_env, TokioIo};
use crate::tunnel::handle_proxy_request;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {



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

    let endpoint_config = quinn::EndpointConfig::default();


    let port = get_value_from_env("QUIC_PORT").unwrap_or(0);
    let endpoint = SocketAddr::from(([0,0,0,0], port));

    let socket = std::net::UdpSocket::bind(endpoint)?;

    println!("Listening on {}", socket.local_addr()?);

    let runtime = Arc::new(TokioRuntime);
    let socket = runtime.wrap_udp_socket(socket)?;
    let endpoint = quinn::Endpoint::new_with_abstract_socket(
        endpoint_config,
        Some(server_config),
        socket,
        runtime
    )?;

    while let Some(conn) = endpoint.accept().await {
        tokio::spawn(async move {
            handle_quic_connection(conn).await.ok();
        });
    }

    Ok(())
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