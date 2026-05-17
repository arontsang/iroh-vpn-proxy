use std::fmt::{Debug, Formatter};
use std::future::poll_fn;
use std::io::IoSliceMut;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use std::time::Duration;
use futures::future::{AbortHandle, Abortable} ;
use quinn::{AsyncUdpSocket, Runtime, UdpPoller};
use quinn::udp::{RecvMeta, Transmit};
use stun::message::Getter;

pub struct StunSocket {
    socket: Arc<dyn AsyncUdpSocket + Send + Sync>,
    stun_socket_addr: RwLock<Option<SocketAddr>>,
    stun_server_poller: AbortHandle,
}

impl Drop for StunSocket {
    fn drop(&mut self) {
        self.stun_server_poller.abort();
    }
}


impl StunSocket {
    pub(crate) fn new(socket_addr: SocketAddr, runtime: Arc<dyn Runtime>) -> anyhow::Result<Self> {
        let socket = std::net::UdpSocket::bind(socket_addr)?;
        let socket = runtime.wrap_udp_socket(socket)?;

        let (abort_handle, abort_registration) = AbortHandle::new_pair();

        let stun_server_poller = Abortable::new({
            let socket = socket.clone();
            let runtime = runtime.clone();
            async move{
                // Wait until the socket is ready
                let mut timer = runtime.new_timer(runtime.now());
                poll_fn(|cx| timer.as_mut().poll(cx)).await;
                loop {
                    let mut request = stun::message::Message::new();
                    request.build(&[
                        Box::new(stun::message::BINDING_REQUEST),
                    ]).unwrap();

                    let request = Transmit {
                        destination: SocketAddr::from(([74, 125, 250, 129], 19302)),
                        ecn: None,
                        contents: request.raw.as_slice(),
                        segment_size: None,
                        src_ip: None,
                    };

                    socket.try_send(&request).ok();


                    println!("Sending to Stun Server");
                    let mut timer = runtime.new_timer(runtime.now() + Duration::from_secs(10));
                    poll_fn(|cx| timer.as_mut().poll(cx)).await;
                }

            }
        }, abort_registration);
        runtime.spawn(Box::pin(async move {
            stun_server_poller.await.ok();
        }));

        Ok(Self{
            socket,
            stun_socket_addr: RwLock::new(None),
            stun_server_poller: abort_handle,
        })

    }
}

impl Debug for StunSocket {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.socket.fmt(f)
    }
}

impl AsyncUdpSocket for StunSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        self.socket.clone().create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit) -> std::io::Result<()> {
        self.socket.try_send(transmit)
    }

    fn poll_recv(&self, cx: &mut Context, bufs: &mut [IoSliceMut<'_>], meta: &mut [RecvMeta]) -> Poll<std::io::Result<usize>> {
        match self.socket.poll_recv(cx, bufs, meta) {
            Poll::Ready(Ok(n)) => {
                for index in 0..n {
                    let meta = &meta[index];
                    let buf: &[u8] = &bufs[index];
                    let buf: &[u8] = &buf[..meta.len];

                    if buf.len() < 20 || &buf[4..8] != &[0x21, 0x12, 0xA4, 0x42] {
                        continue;
                    }
                    println!("Received from Stun Server");
                    let mut response = stun::message::Message::new();
                    let mut xor_addr = stun::xoraddr::XorMappedAddress::default();
                    if let Ok(_) = response.write(buf) && let Ok(()) =xor_addr.get_from(&response) {
                        println!("Your public IP and Port: {}:{}", xor_addr.ip, xor_addr.port);

                        if let Ok(mut guard) = self.stun_socket_addr.write() {
                            *guard = Some(SocketAddr::from((xor_addr.ip, xor_addr.port)));
                            println!("Updated public IP and Port: {}:{}", xor_addr.ip, xor_addr.port);
                        }
                    }
                }

                Poll::Ready(Ok(n))
            },
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        if let Ok(guard) = self.stun_socket_addr.read() {
            if let Some(addr) = guard.as_ref() {
                return Ok(*addr);
            }
        }
        self.socket.local_addr()
    }
}