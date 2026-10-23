use crate::support::get_value_from_env;
use crate::support::iroh::{STUN_QUIC_ALPN, build_endpoint};
use async_executor::LocalExecutor;
use iroh::Endpoint;
use iroh::endpoint::{Connection, OpenBi};
use iroh_tickets::endpoint::EndpointTicket;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::str::FromStr;
use std::time::Duration;

pub struct Uplink {
    pub connection: Connection,
    _endpoint: Endpoint,
}

impl Uplink {
    pub fn open_bi(&self) -> OpenBi<'_> {
        self.connection.open_bi()
    }
}

impl Drop for Uplink {
    fn drop(&mut self) {
        println!("Dropping uplink")
    }
}

pub struct IrohConnectionPool<'a> {
    client: reqwest::Client,
    pool_item: RefCell<Weak<Uplink>>,
    creation_lock: tokio::sync::Mutex<()>,
    pub local_executor: Rc<LocalExecutor<'a>>,
}



impl<'a> IrohConnectionPool<'a> {
    
    pub fn new(local_executor: Rc<LocalExecutor<'a>>) -> Self {
        Self {
            client: reqwest::Client::default(),
            pool_item: RefCell::new(Weak::default()),
            creation_lock: Default::default(),
            local_executor
        }
    }

    pub async fn get(&self) -> anyhow::Result<Rc<Uplink>> {
        if let Some(value) = self.pool_item.borrow().upgrade() {
            Ok(value)
        } else {
            let _write_guard = self.creation_lock.lock().await;
            if let Some(value) = self.pool_item.borrow().upgrade() {
                Ok(value)
            } else {
                Ok(self.build_new_connection().await?)
            }
        }
    }

    async fn get_ticket(&self, server_base: &String) -> anyhow::Result<String> {
        let request = self.client.get(&format!("{}/iroh/ticket", server_base))
            .header("Cache-Control", "no-cache, no-store, must-revalidate")
            .header("Pragma", "no-cache")
            .header("Expires", "0")
            .send().await?;
        let request = match request.error_for_status() {
            Ok(request) => Ok(request),
            Err(e) => {
                // Error handling stolen from AI
                if e.is_builder()       { println!("Failed to build the client or request."); }
                if e.is_request()       { println!("Failed to send the request."); }
                if e.is_body()          { println!("Error with request or response body."); }
                if e.is_decode()        { println!("Failed to deserialize the response body."); }
                if e.is_redirect()      { println!("Error following standard HTTP redirects."); }
                if e.is_timeout()       { println!("The request timed out."); }

                // Check if the error was triggered by an HTTP status code (e.g. res.error_for_status())
                if let Some(status) = e.status() {
                    println!("HTTP Status Error code: {}", status);
                }

                // Retrieve the URL related to this error if present
                if let Some(url) = e.url() {
                    println!("Error occurred while visiting: {}", url);
                }
                Err(e)
            }
        }?;

        Ok(request.text().await?)
    }

    async fn build_new_connection(&self) -> anyhow::Result<Rc<Uplink>> {
        println!("Building new connection");
        let server_base = std::env::var("SERVER_ADDR")?;
        let ticket = self.get_ticket(&server_base).await?;
        println!("Got IROH ticket {}", ticket);

        let endpoint = build_endpoint().await?;
        println!("Built endpoint {}", endpoint.id());
        let ticket = EndpointTicket::from_str(&ticket)?;
        let connection = endpoint.connect(ticket, STUN_QUIC_ALPN.as_bytes()).await;

        println!("Begin connecting...");
        let connection = match connection {
            Ok(x) => { x }
            Err(err) => {
                println!("Failed to connect to {}: {}", server_base, err);
                return Err(anyhow::anyhow!(err));
            }
        };

        println!("connected to server {}", server_base);

        let uplink = Rc::new(Uplink { connection, _endpoint: endpoint });

        self.pool_item.replace(Rc::downgrade(&uplink.clone()));

        let keep_alive = self.local_executor.spawn(async move {
            loop {
                // Continuously ping the server to keep the connection alive.
                println!("Calling GET /keep_alive");
                reqwest::get(&format!("{}/keep_alive?time_out_secs=300", server_base)).await.ok();
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });


        self.local_executor.spawn({
            let uplink = uplink.clone();
            let keep_alive = keep_alive;
            let linger = get_value_from_env("LINGER_SECS").unwrap_or(10u16);
            async move {
                let mut dead_count = 0u16;
                while dead_count < linger {
                    // Check every 1 second if we hold the last reference to the connection.
                    tokio::time::sleep(Duration::from_secs(1)).await;

                    if Rc::strong_count(&uplink) == 1 {
                        dead_count += 1;
                    } else {
                        dead_count = 0;
                    }
                }

                println!("Closing connection after {} seconds of inactivity", linger);
                // We are the last reference, close the connection.
                drop(uplink);
                drop(keep_alive)
            }
        }).detach();
        Ok(uplink)
    }
}
