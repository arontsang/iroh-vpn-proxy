mod support;

use std::cell::Cell;
use std::io::IoSlice;
use crate::support::connection_pool::IrohConnectionPool;
use crate::support::get_value_from_env;
use anyhow::Result;
use async_executor::LocalExecutor;

use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use pin_project::pin_project;
use tokio::io::{copy_bidirectional, AsyncRead, AsyncWrite, ReadBuf, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};


async fn handle_incoming(incoming: TcpStream, pool: Rc<IrohConnectionPool<'_>>) -> Result<()> {
    let handler = pool.get().await?;
    let (send, recv) = handler.open_bi().await?;

    let upstream = tokio::io::join(recv, send);

    let last_updated = Cell::new(Instant::now());

    let mut upstream = LastUpdatedStream { inner: upstream, last_updated: &last_updated };
    let mut incoming = LastUpdatedStream { inner: incoming, last_updated: &last_updated };

    let timeout = async {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let time_since_last_update = Instant::now() - last_updated.get();
            if time_since_last_update > Duration::from_secs(20) {
                break;
            }
        }
    };

    tokio::select! {
        _ = copy_bidirectional(&mut upstream, &mut incoming) => {},
        _ = timeout => {}
    }

    upstream.shutdown().await.ok();
    incoming.shutdown().await.ok();

    Ok(())
}


#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let local_ex = Rc::new(LocalExecutor::new());
    let listener = open_tcp_listener()?;

    let pool = Rc::new(IrohConnectionPool::new(local_ex.clone()));

    local_ex.run(async {
        loop {
            let local_ex = local_ex.clone();
            if let Ok((incoming, client_addr)) = listener.accept().await {                
                println!("Incoming connection from {}", client_addr);
                let pool = pool.clone();

                local_ex.spawn(async move {
                    handle_incoming(incoming, pool).await.ok();
                }).detach();
            }
        }
    }).await;
    Ok(())
}

fn open_tcp_listener() -> Result<TcpListener> {
    // We want to use SO_REUSE because we can then
    // set host networking true on the kubernetes
    // deployment without fear of deployment rollouts

    // We want host networking to get around Calico's
    // Symmetric NAT.
    // By using host networking, we can get iroh
    // to UDP NAT holepunch for better performance
    // and reliablity.

    let addr = SocketAddr::from(([0, 0, 0, 0], get_value_from_env::<u16>("PROXY_PORT").unwrap_or(0)));
    let domain = socket2::Domain::for_address(addr);
    let socket = socket2::Socket::new(domain, socket2::Type::STREAM, None)?;

    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_reuse_address(true)?;

    socket.bind(&addr.into())?;
    socket.listen(1024)?;

    socket.set_nonblocking(true)?;
    let listener = std::net::TcpListener::from(socket);
    Ok(TcpListener::from_std(listener)?)
}

#[pin_project]
struct LastUpdatedStream<'a, T> {
    #[pin]
    inner: T,

    last_updated: &'a Cell<Instant>
}

impl<T : AsyncRead> AsyncRead for LastUpdatedStream<'_, T> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_read(cx, buf)
    }
}

impl<T : AsyncWrite> AsyncWrite for LastUpdatedStream<'_, T> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let this = self.project();
        match this.inner.poll_write(cx, buf) {
            Poll::Ready(Ok(bytes_written)) if bytes_written > 0 => {
                this.last_updated.set(Instant::now());
                Poll::Ready(Ok(bytes_written))
            }
            result => result,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }

    fn poll_write_vectored(self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[IoSlice<'_>]) -> Poll<std::io::Result<usize>> {
        let this = self.project();
        match this.inner.poll_write_vectored(cx, bufs) {
            Poll::Ready(Ok(bytes_written)) if bytes_written > 0 => {
                this.last_updated.set(Instant::now());
                Poll::Ready(Ok(bytes_written))
            }
            result => result,
        }
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}
