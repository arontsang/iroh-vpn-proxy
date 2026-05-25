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
    let socket = SocketAddr::from(([0, 0, 0, 0], get_value_from_env::<u16>("PROXY_PORT").unwrap_or(0)));
    let socket = TcpListener::bind(socket).await?;

    let pool = Rc::new(IrohConnectionPool::new(local_ex.clone()));

    local_ex.run(async {
        loop {
            let local_ex = local_ex.clone();
            if let Ok((incoming, _)) = socket.accept().await {
                let pool = pool.clone();

                local_ex.spawn(async move {
                    handle_incoming(incoming, pool).await.ok();
                }).detach();
            }
        }
    }).await;
    Ok(())
}



