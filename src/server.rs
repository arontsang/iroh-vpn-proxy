pub mod support;
pub mod tunnel;
mod web;

use std::env;
use std::pin::Pin;
use crate::support::{get_value_from_env, TokioIo};
use crate::tunnel::handle_proxy_request;
use crate::web::handle_web_request;
use anyhow::Result;
use std::sync::Arc;
use iroh::Endpoint;
use iroh::endpoint::{presets, Connection, QuicTransportConfig};
use iroh::protocol::{AcceptError, DynProtocolHandler, Router};
use iroh_tickets::endpoint::EndpointTicket;
use crate::support::iroh::build_endpoint;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let endpoint = build_endpoint().await?;
    endpoint.online().await;

    // Optionally push endpoint metrics to iroh-services if an API secret is
    // available. Keep the client bound for the lifetime of the receiver so it
    // continues reporting in the background.
    let _services_client = match env::var("IROH_SERVICES_API_SECRET") {
        Ok(_) => {
            let client = iroh_services::Client::builder(&endpoint)
                .api_secret_from_env()?
                .name("iroh-vpn-proxy")?
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
    let _router = Router::builder(endpoint)
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
        println!("new connection from {}", connection.remote_id());
        Box::pin(async move {

            tokio::spawn( {
                let connection = connection.clone();
                async move {
                    let error = connection.closed().await;
                    println!("connection closed: {:?}", error);
                }
            });

            // select! {
            //     _ = async {
            //         while let Ok((send, recv)) = connection.accept_bi().await {
            //             println!("accepted connection from {}", connection.remote_id());
            //             let client = tokio::io::join(recv, send);
            //             let client = TokioIo::new(client);
            //             _ = tokio::spawn(async move {
            //                 handle_proxy_request(client);
            //             });
            //         }
            //     } => {},
            //     x = connection.closed() => { println!("client disconnected: {:?}", x); },
            // }

            while let Ok((send, recv)) = connection.accept_bi().await {
                println!("accepted connection from {}", connection.remote_id());
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

