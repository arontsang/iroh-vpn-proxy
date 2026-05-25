use iroh::Endpoint;
use iroh::endpoint::{presets, QuicTransportConfig};
use crate::support::get_value_from_env;

pub async fn build_endpoint() -> anyhow::Result<Endpoint> {
    let mut quic_config = QuicTransportConfig::builder();
    if let Some(value) = get_value_from_env::<u32>("QUIC_RECV_WINDOW") {
        quic_config = quic_config.receive_window(value.into());
    };
    if let Some(value) = get_value_from_env::<u32>("QUIC_SEND_WINDOW") {
        quic_config = quic_config.send_window(value.into());
    };
    if let Some(value) = get_value_from_env::<u32>("QUIC_STREAM_WINDOW") {
        quic_config = quic_config.stream_receive_window(value.into());
    };
    let quic_config = quic_config.build();

    let endpoint = Endpoint::builder(presets::N0)
        .transport_config(quic_config)
        .bind().await?;

    Ok(endpoint)
}

pub const STUN_QUIC_ALPN: &'static str = "stun-quic";

