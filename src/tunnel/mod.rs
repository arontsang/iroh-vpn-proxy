use anyhow::Result;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::client::conn::http1::Builder as ClientBuilder;
use hyper::server::conn::http1::Builder as ServerBuilder;
use hyper::service::service_fn;
use hyper::upgrade::Upgraded;
use hyper::{Method, Request, Response};
use hyper::rt::{Read, Write};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{self, Duration};

pub use crate::support::get_value_from_env;

pub mod io_adaptor;
use crate::tunnel::io_adaptor::TokioIo;

pub fn handle_proxy_request<TRequest>(request: TRequest)
where TRequest : Read + Write + Unpin + Send + 'static {
    tokio::spawn(async move {
        if let Err(_) = ServerBuilder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .serve_connection(request, service_fn(proxy))
            .with_upgrades()
            .await
        {
        }
    });
}

async fn proxy(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    //println!("req: {:?}", req);

    if Method::CONNECT == req.method() {
        // Received an HTTP request like:
        // ```
        // CONNECT www.domain.com:443 HTTP/1.1
        // Host: www.domain.com:443
        // Proxy-Connection: Keep-Alive
        // ```
        //
        // When HTTP method is CONNECT, we should return an empty body,
        // then we can eventually upgrade the connection and talk a new protocol.
        //
        // Note: only after the client received an empty body with STATUS_OK can the
        // connection be upgraded, so we can't return a response inside
        // `on_upgrade` future.
        if let Some(addr) = host_addr(req.uri()) {
            tokio::task::spawn(async move {
                match hyper::upgrade::on(req).await {
                    Ok(upgraded) => {
                        if let Err(e) = tunnel(upgraded, addr).await {
                            eprintln!("server io error: {}", e);
                        };
                    }
                    Err(e) => eprintln!("upgrade error: {}", e),
                }
            });

            Ok(Response::new(empty()))
        } else {
            eprintln!("CONNECT host is not socket addr: {:?}", req.uri());
            let mut resp = Response::new(full("CONNECT must be to a socket address"));
            *resp.status_mut() = http::StatusCode::BAD_REQUEST;

            Ok(resp)
        }
    } else {
        let host = req.uri().host().expect("uri has no host");
        let port = req.uri().port_u16().unwrap_or(80);

        let stream = TcpStream::connect((host, port)).await.unwrap();
        let io = TokioIo::new(stream);

        let (mut sender, conn) = ClientBuilder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .handshake(io)
            .await?;
        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });

        let resp = sender.send_request(req).await?;
        Ok(resp.map(|b| b.boxed()))
    }
}

fn host_addr(uri: &http::Uri) -> Option<String> {
    uri.authority().map(|auth| auth.to_string())
}

fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

async fn tunnel(upgraded: Upgraded, addr: String) -> std::io::Result<()> {
    // Connect to remote server
    println!("opening tunnel to {addr}");
    let mut server = TcpStream::connect(addr.as_str()).await?;
    server.set_nodelay(true)?;
    let upgraded = TokioIo::new(upgraded);

    let buffer_size: usize = get_value_from_env("PROXY_BUFFER_SIZE").unwrap_or(8 * 1024);
    let idle_timeout_secs: u64 = get_value_from_env("PROXY_IDLE_TIMEOUT_SECS").unwrap_or(30);
    let idle = Duration::from_secs(idle_timeout_secs);

    let (ur, uw) = tokio::io::split(upgraded);
    let (sr, sw) = tokio::io::split(server);

    let a = copy_idle(ur, sw, buffer_size, idle);
    let b = copy_idle(sr, uw, buffer_size, idle);

    let conn_timeout = Duration::from_secs(5 * 60);

    match time::timeout(conn_timeout, async {
        tokio::select! {
            r = a => { r?; }
            r = b => { r?; }
        }
        Ok(())
    })
    .await
    {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => return Err(e),
        Err(_elapsed) => {
            eprintln!("tunnel to {addr}: connection timeout after 5 minutes");
            Ok(())
        }
    }
}

async fn copy_idle<R, W>(
    mut reader: R,
    mut writer: W,
    buffer_size: usize,
    idle_timeout: Duration,
) -> std::io::Result<u64>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; buffer_size];
    let mut total = 0u64;
    loop {
        let n = match time::timeout(idle_timeout, reader.read(&mut buf)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(e),
            Err(_elapsed) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "idle timeout",
                ));
            }
        };
        total += n as u64;
        writer.write_all(&buf[..n]).await?;
    }
    writer.shutdown().await?;
    Ok(total)
}

