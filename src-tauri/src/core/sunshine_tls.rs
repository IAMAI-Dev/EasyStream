use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring, verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};

#[derive(Debug)]
struct SunshineCertificate {
    leaf: CertificateDer<'static>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for SunshineCertificate {
    fn verify_server_cert(
        &self, leaf: &CertificateDer<'_>, _: &[CertificateDer<'_>],
        _: &ServerName<'_>, _: &[u8], _: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if leaf.as_ref() != self.leaf.as_ref() {
            return Err(Error::General("Sunshine TLS certificate does not match the local pin".into()));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, signature: &DigitallySignedStruct)
        -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, cert, signature, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, signature: &DigitallySignedStruct)
        -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, signature, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

pub(super) fn pinned_config(pem: &[u8]) -> Result<ClientConfig, String> {
    let leaf = rustls_pemfile::certs(&mut &pem[..]).next()
        .ok_or("Sunshine PEM 文件缺少证书")?.map_err(|e| e.to_string())?;
    let provider = Arc::new(ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions().map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SunshineCertificate { leaf, provider }))
        .with_no_client_auth();
    Ok(config)
}
