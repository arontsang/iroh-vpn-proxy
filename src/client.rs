mod stun;
mod support;
mod web;

use crate::support::get_value_from_env;
use anyhow::Result;
use iroh::endpoint::{presets, Connection, VarInt};
use iroh::Endpoint;
use iroh_tickets::endpoint::EndpointTicket;
use pin_project::__private::PinnedDrop;
use pin_project::pin_project;
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
use tokio::task::{JoinHandle, LocalSet};

#[pin_project(PinnedDrop)]
struct CopyJob<F> {
    #[pin]
    future: F,
    handler: Rc<Uplink>,
    local: Rc<LocalSet>,

    linger: Duration
}

struct Uplink {
    connection: Connection,
    _endpoint: Endpoint,
    keep_alive: JoinHandle<()>,
}

impl Drop for Uplink {
    fn drop(&mut self) {
        self.keep_alive.abort();
        self.connection.close(VarInt::default(), "Dropping Uplink".as_bytes());
        println!("dropping uplink");
    }
}

impl<F> PinnedDrop for CopyJob<F> {
    unsafe fn drop(self: Pin<&mut Self>) {
        self.local.spawn_local({
            let handler = self.handler.clone();
            let linger = self.linger.clone();
            async move {
                tokio::time::sleep(linger).await;
                drop(handler);
            }
        });
    }
}

impl<F: Future<Output = Ret>, Ret> Future for CopyJob<F> {
    type Output = Ret;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        this.future.poll(cx)
    }
}



async fn build_new_connection() -> Result<Uplink> {
    let server_base = std::env::var("SERVER_ADDR")?;
    let ticket = reqwest::get(&format!("{}/iroh/ticket", server_base)).await?
        .text()
        .await?;

    let endpoint = Endpoint::bind(presets::N0).await?;
    let ticket = EndpointTicket::from_str(&ticket)?;
    let connection = endpoint.connect(ticket, "stun-proxy".as_bytes()).await?;
    let keep_alive = tokio::spawn(async move {
        loop {
            // Continously ping the server to keep the connection alive.
            reqwest::get(&format!("{}/keep_alive?time_out_secs=300", server_base)).await.ok();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
    Ok(Uplink { connection, _endpoint: endpoint, keep_alive })
}

async fn handle_incoming(mut incoming: TcpStream, app: AppState) -> Result<()> {
    let handler = app.get_uplink().await?;
    let (send, recv) = handler.connection.open_bi().await?;

    let mut upstream = tokio::io::join(recv, send);
    CopyJob {
        future: async move { copy_bidirectional(&mut incoming, &mut upstream).await },
        local: app.local_set.clone(),
        linger: app.linger,
        handler,
    }.await?;

    Ok(())
}

#[derive(Clone)]
struct AppState {
    handler_pool: Rc<Mutex<Cell<Weak<Uplink>>>>,
    local_set: Rc<LocalSet>,
    linger: Duration,
}

impl AppState {
    async fn get_uplink(&self) -> Result<Rc<Uplink>> {

        let mut handler = self.handler_pool.lock().await;

        if let Some(handler) = handler.get_mut().upgrade() {
            return Ok(handler);
        }

        let connection = build_new_connection().await?;

        let ret = Rc::new(connection);
        handler.set(Rc::downgrade(&ret.clone()));
        Ok(ret)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let local_set = Rc::new(LocalSet::new());
    let handler: Rc<Mutex<Cell<Weak<Uplink>>>> = Rc::new(Mutex::new(Cell::new(Weak::default())));
    let socket = SocketAddr::from(([0, 0, 0, 0], get_value_from_env::<u16>("PROXY_PORT").unwrap_or(0)));
    let socket = TcpListener::bind(socket).await?;

    let linger = Duration::from_secs(get_value_from_env("LINGER_SECS").unwrap_or(10u64));

    let app = AppState {
        linger,
        local_set: local_set.clone(),
        handler_pool: handler,
    };

    local_set.run_until(async {
        loop {
            if let Ok((incoming, _)) = socket.accept().await {
                let app = app.clone();
                local_set.spawn_local(async { handle_incoming(incoming, app).await.ok() });
            }
        }
    }).await;
    Ok(())
}



