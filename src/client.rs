mod support;

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use anyhow::Result;
use quinn::{ClientConfig, Endpoint};
use quinn::crypto::rustls::QuicClientConfig;
use tokio::net::{TcpListener};
use crate::support::{get_value_from_env, SkipServerVerification};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let address = std::env::var("SERVER_ADDR")
        .unwrap()
        .to_socket_addrs()?
        .next()
        .unwrap();
    let mut endpoint = Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;

    let client_config = ClientConfig::new(Arc::new(QuicClientConfig::try_from(
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth(),
    )?));
    endpoint
        .set_default_client_config(client_config);
    let connection = endpoint
        .connect(address, "localhost")?
        .await?;

    loop {

        let socket = SocketAddr::from(([0, 0, 0, 0], get_value_from_env::<u16>("PROXY_PORT").unwrap_or(0)));
        let socket = TcpListener::bind(socket).await?;

        loop {
            let (mut incoming, _) = socket.accept().await?;
            tokio::spawn(async move {

                let (send, recv) = connection.open_bi().await.unwrap();
                let mut upstream = tokio::io::join(recv, send);
                tokio::io::copy_bidirectional(&mut incoming, &mut upstream).await.ok();
            });
        }

    }
}

