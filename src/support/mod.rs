
//! Various runtimes for hyper

pub mod skip_server_verification;
pub mod io_adaptor;
pub mod static_cert_verifier;

pub use io_adaptor::*;

pub use skip_server_verification::*;
