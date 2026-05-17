use std::sync::Arc;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified};
use rustls::{CertificateError, DigitallySignedStruct, Error, SignatureScheme};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

#[derive(Debug)]
pub struct StaticCertServerVerification<'a> {
    rustls: Arc<rustls::crypto::CryptoProvider>,
    certificate: CertificateDer<'a>,
}

impl<'a> StaticCertServerVerification<'a> {
    pub fn new(certificate: CertificateDer<'a>) -> Arc<Self> {
        Arc::new(Self{
            rustls: Arc::new(rustls::crypto::ring::default_provider()),
            certificate,
        })
    }
}

impl rustls::client::danger::ServerCertVerifier for StaticCertServerVerification<'_> {
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>], server_name: &ServerName<'_>, ocsp_response: &[u8], now: UnixTime) -> Result<ServerCertVerified, Error> {
        if &self.certificate == end_entity {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::InvalidCertificate(CertificateError::InvalidPurpose))
        }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.rustls.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.rustls.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.rustls.signature_verification_algorithms.supported_schemes()
    }
}