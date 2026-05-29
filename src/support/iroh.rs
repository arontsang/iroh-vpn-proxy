use std::sync::Arc;
use iroh::Endpoint;
use iroh::endpoint::{presets, QuicTransportConfig};
use noq_proto::congestion::{BbrConfig, ControllerFactory};
use crate::support::get_value_from_env;


fn get_congestion_controller() -> Arc<dyn ControllerFactory + Send + Sync + 'static> {
    let mut ret = BbrConfig::default();
    if let Some(value) = get_value_from_env("QUIC_BBR_INITIAL_WINDOW") {
        ret.initial_window(value);
    }

    Arc::new(ret)
}

pub async fn build_endpoint() -> anyhow::Result<Endpoint> {
    let mut quic_config = QuicTransportConfig::builder();
    quic_config = quic_config.congestion_controller_factory(get_congestion_controller());

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


    let mut endpoint = Endpoint::builder(presets::N0)
        .transport_config(quic_config);

    if let Some(bind_addr) = get_value_from_env::<String>("BIND_ADDR") {
        endpoint = endpoint.bind_addr(bind_addr)?;
    }

    let endpoint = endpoint
        .bind()
        .await?;

    Ok(endpoint)
}

pub const STUN_QUIC_ALPN: &'static str = "stun-quic";

