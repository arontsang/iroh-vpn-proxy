mod support;

use crate::support::connection_pool::IrohConnectionPool;
use crate::support::get_value_from_env;
use anyhow::Result;
use async_executor::LocalExecutor;

use std::net::SocketAddr;
use std::rc::Rc;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};


async fn handle_incoming(mut incoming: TcpStream, pool: Rc<IrohConnectionPool<'_>>) -> Result<()> {
    let handler = pool.get().await?;
    let (send, recv) = handler.open_bi().await?;

    let mut upstream = tokio::io::join(recv, send);

    copy_bidirectional(&mut upstream, &mut incoming).await?;

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
            if let Ok((incoming, _)) = listener.accept().await {
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


