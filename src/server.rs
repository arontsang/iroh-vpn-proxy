pub mod stun;
pub mod support;
pub mod tunnel;
mod web;

use std::env;
use std::pin::Pin;
use crate::support::TokioIo;
use crate::tunnel::handle_proxy_request;
use crate::web::handle_web_request;
use anyhow::Result;
use std::sync::Arc;
use iroh::Endpoint;
use iroh::endpoint::{presets, Connection};
use iroh::protocol::{AcceptError, DynProtocolHandler, Router};
use iroh_tickets::{Ticket, endpoint::EndpointTicket};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let endpoint = Endpoint::bind(presets::N0).await?;
    endpoint.online().await;

    // Optionally push endpoint metrics to iroh-services if an API secret is
    // available. Keep the client bound for the lifetime of the receiver so it
    // continues reporting in the background.
    let _services_client = match env::var("IROH_SERVICES_API_SECRET") {
        Ok(_) => {
            let client = iroh_services::Client::builder(&endpoint)
                .api_secret_from_env()?
                .name("iroh-ping-quickstart")?
                .build()
                .await?;
            println!("registered with iroh-services, pushing endpoint metrics");
            Some(client)
        }
        Err(_) => {
            println!(
                "IROH_SERVICES_API_SECRET not set, skipping iroh-services setup. \
                 Get a free API key at https://services.iroh.computer to see endpoint metrics and debug connectivity issues."
            );
            None
        }
    };

    let ticket = EndpointTicket::new(endpoint.addr());

    let handler: Box<dyn DynProtocolHandler> = Box::new(ProxyHandler);
    let router = Router::builder(endpoint)
        .accept("stun-proxy".as_bytes(), handler)
        .spawn();


    tokio::select! {
        _ = handle_web_request(Arc::new(ticket.into())) => {},
        _ = tokio::signal::ctrl_c() => {}
    }

    Ok(())
}


#[derive(Debug)]
struct ProxyHandler;

impl DynProtocolHandler for ProxyHandler {
    fn accept(&self, connection: Connection) -> Pin<Box<dyn Future<Output=std::result::Result<(), AcceptError>> + Send + '_>> {
        Box::pin(async move {
            while let Ok((send, recv)) = connection.accept_bi().await {
                let client = tokio::io::join(recv, send);
                let client = TokioIo::new(client);
                _ = tokio::spawn(async move {
                    handle_proxy_request(client);
                });
            }
            Ok(())
        })
    }
}

