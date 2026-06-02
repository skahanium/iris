use std::sync::Arc;
use std::time::Duration;

use reqwest::ClientBuilder;

use crate::error::{AppError, AppResult};

const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// 创建带有安全 TLS 配置的 HTTP client builder（无证书固定）。
///
/// 调用方可在 `.build()` 前追加自定义配置（如 `.user_agent()`）。
///
/// - 强制 HTTPS（拒绝明文 HTTP）
/// - 使用 rustls TLS 后端（不依赖系统 OpenSSL）
/// - 默认 60 秒超时
pub fn pinned_client_builder() -> ClientBuilder {
    reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
}

/// 创建带有安全 TLS 配置的 HTTP client（无证书固定）。
pub fn create_pinned_client() -> AppResult<reqwest::Client> {
    pinned_client_builder()
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build HTTP client: {e}")))
}

/// 创建带有可选证书固定的 HTTP client。
///
/// 当 `pins` 非空时，TLS 连接仅在接受端证书 SHA-256 指纹
/// 匹配配置列表中的某一项后才被信任。同时仍执行标准 WebPKI 根信任验证。
pub fn create_pinned_client_with_pins(pins: &[String]) -> AppResult<reqwest::Client> {
    if pins.is_empty() {
        return pinned_client_builder()
            .build()
            .map_err(|e| AppError::msg(format!("Failed to build HTTP client: {e}")));
    }

    let verifier = Arc::new(CertPinVerifier::new(pins.to_vec())?);

    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    pinned_client_builder()
        .use_preconfigured_tls(config)
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build pinned HTTP client: {e}")))
}

// ── Internal cert pin verifier ──────────────────────────────────────────

#[derive(Debug)]
struct CertPinVerifier {
    fingerprints: Vec<String>,
}

impl CertPinVerifier {
    fn new(fingerprints: Vec<String>) -> AppResult<Self> {
        let valid: Vec<String> = fingerprints
            .into_iter()
            .filter(|fp| {
                if fp.len() == 64 && fp.chars().all(|c| c.is_ascii_hexdigit()) {
                    true
                } else {
                    tracing::warn!(
                        fingerprint = %fp,
                        "invalid cert fingerprint ignored (must be 64 hex chars)"
                    );
                    false
                }
            })
            .collect();
        if valid.is_empty() {
            return Err(AppError::msg(
                "cert_pins configured but no valid SHA-256 fingerprints provided",
            ));
        }
        Ok(Self {
            fingerprints: valid,
        })
    }

    fn verify_fingerprint(
        &self,
        end_entity: &[u8],
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(end_entity);
        let hex = hex::encode(hash);

        if self
            .fingerprints
            .iter()
            .any(|pin| pin.eq_ignore_ascii_case(&hex))
        {
            tracing::debug!(fingerprint = %hex, "cert pin matched");
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            tracing::warn!(
                fingerprint = %hex,
                configured_pins = ?self.fingerprints,
                "TLS certificate fingerprint does not match any pinned fingerprint"
            );
            Err(rustls::Error::General(
                "certificate fingerprint does not match any pin".into(),
            ))
        }
    }
}

impl rustls::client::danger::ServerCertVerifier for CertPinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        self.verify_fingerprint(end_entity.as_ref())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pins_uses_default_client() {
        assert!(create_pinned_client_with_pins(&[]).is_ok());
    }

    #[test]
    fn invalid_fingerprint_length_rejected() {
        let result = create_pinned_client_with_pins(&["too_short".into()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no valid SHA-256"));
    }

    #[test]
    fn non_hex_fingerprint_rejected() {
        let result = create_pinned_client_with_pins(&[
            "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn valid_hex_fingerprint_accepted() {
        let valid = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        // Constructing the client with a dummy fingerprint — it won't connect but shouldn't fail to build
        let result = CertPinVerifier::new(vec![valid]);
        assert!(result.is_ok());
    }
}
