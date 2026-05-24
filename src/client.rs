mod support;

use crate::support::get_value_from_env;
use anyhow::Result;
use async_executor::{LocalExecutor, Task};
use iroh::endpoint::{presets, Connection, VarInt};
use iroh::Endpoint;
use iroh_tickets::endpoint::EndpointTicket;

use pin_project::{pin_project, pinned_drop};
use std::cell::Cell;
use std::net::{SocketAddr};
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[pin_project(PinnedDrop)]
struct CopyJob<F> {
    #[pin]
    future: F,
    handler: Rc<Uplink>,
    local_executor: Rc<LocalExecutor<'static>>,

    linger: Duration
}

struct Uplink {
    connection: Connection,
    _endpoint: Endpoint,
    _keep_alive: Task<()>,
}

impl Drop for Uplink {
    fn drop(&mut self) {
        self.connection.close(VarInt::default(), "Dropping Uplink".as_bytes());
        println!("dropping uplink");
    }
}

#[pinned_drop]
impl<F> PinnedDrop for CopyJob<F> {
    fn drop(self: Pin<&mut Self>) {
        self.local_executor.spawn({
            let handler = self.handler.clone();
            let linger = self.linger.clone();
            async move {
                tokio::time::sleep(linger).await;
                drop(handler);
            }
        }).detach();
    }
}

impl<F: Future<Output = Ret>, Ret> Future for CopyJob<F> {
    type Output = Ret;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        this.future.poll(cx)
    }
}





async fn handle_incoming(mut incoming: TcpStream, app: AppState) -> Result<()> {
    let handler = app.get_uplink().await?;
    let (send, recv) = handler.connection.open_bi().await?;

    let mut upstream = tokio::io::join(recv, send);
    CopyJob {
        future: async move { copy_bidirectional(&mut incoming, &mut upstream).await },
        local_executor: app.local_executor.clone(),
        linger: app.linger,
        handler,
    }.await?;

    Ok(())
}

#[derive(Clone)]
struct AppState {
    handler_pool: Rc<Mutex<Cell<Weak<Uplink>>>>,
    local_executor: Rc<LocalExecutor<'static>>,
    linger: Duration,
}

impl AppState {
    async fn get_uplink(&self) -> Result<Rc<Uplink>> {

        let mut handler = self.handler_pool.lock().await;

        if let Some(handler) = handler.get_mut().upgrade() {
            return Ok(handler);
        }

        let connection = self.build_new_connection().await?;

        let ret = Rc::new(connection);
        handler.set(Rc::downgrade(&ret.clone()));
        Ok(ret)
    }

    async fn build_new_connection(&self) -> Result<Uplink> {
        let server_base = std::env::var("SERVER_ADDR")?;
        let ticket = reqwest::get(&format!("{}/iroh/ticket", server_base)).await?
            .text()
            .await?;

        let endpoint = Endpoint::bind(presets::N0).await?;
        let ticket = EndpointTicket::from_str(&ticket)?;
        let connection = endpoint.connect(ticket, "stun-proxy".as_bytes()).await?;
        println!("connected to server {}", server_base);
        let keep_alive = self.local_executor.spawn(async move {
            loop {
                // Continuously ping the server to keep the connection alive.
                println!("Calling GET /keep_alive");
                reqwest::get(&format!("{}/keep_alive?time_out_secs=300", server_base)).await.ok();
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        Ok(Uplink { connection, _endpoint: endpoint, _keep_alive: keep_alive })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let local_ex = Rc::new(LocalExecutor::new());
    let handler: Rc<Mutex<Cell<Weak<Uplink>>>> = Rc::new(Mutex::new(Cell::new(Weak::default())));
    let socket = SocketAddr::from(([0, 0, 0, 0], get_value_from_env::<u16>("PROXY_PORT").unwrap_or(0)));
    let socket = TcpListener::bind(socket).await?;

    let linger = Duration::from_secs(get_value_from_env("LINGER_SECS").unwrap_or(10u64));

    let app = AppState {
        linger,
        local_executor: local_ex.clone(),
        handler_pool: handler,
    };


    local_ex.run(async {
        loop {
            let local_ex = local_ex.clone();
            if let Ok((incoming, _)) = socket.accept().await {
                let app = app.clone();

                local_ex.spawn(async move {
                    handle_incoming(incoming, app).await.ok();
                }).detach();
            }
        }
    }).await;
    Ok(())
}



