// TLS 璇佷功绠＄悊鍜?HTTP CONNECT 闅ч亾锛堢畝鍖栫増锛夈€?
use std::fs;
use std::sync::Arc;
use std::path::PathBuf;
use log::{debug, error, info, warn};
use rcgen::{CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use crate::modules::config::*;

const SAN_DOMAINS: &[&str] = &[
    "localhost", "api.openai.com", "auth.openai.com", "chat.openai.com",
    "chatgpt.com", "ab.chatgpt.com", "api.anthropic.com", "api.deepseek.com",
];

pub fn ensure_certs() {
    if CERT_FILE.exists() && KEY_FILE.exists() {
        return;
    }
    let _ = fs::create_dir_all(CERT_DIR.as_path());
    match generate_cert_rcgen() {
        Ok(_) => info!("Generated TLS cert chain: {:?}", *CERT_FILE),
        Err(e) => warn!("Certificate generation failed: {}. TLS may not work.", e),
    }
}

fn generate_cert_rcgen() -> Result<(), Box<dyn std::error::Error>> {
    let now = time::OffsetDateTime::now_utc();
    let ca_key = KeyPair::generate()?;

    let mut ca_params = CertificateParams::new(
        SAN_DOMAINS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    )?;
    let mut ca_dn = DistinguishedName::new();
    ca_dn.push(DnType::CommonName, "JinDX-CA");
    ca_dn.push(DnType::OrganizationName, "JinDX Proxy");
    ca_params.distinguished_name = ca_dn;
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca_params.not_before = now;
    ca_params.not_after = now + time::Duration::days(1825);
    let ca_cert = ca_params.self_signed(&ca_key)?;

    let server_key = KeyPair::generate()?;
    let mut server_params = CertificateParams::new(
        SAN_DOMAINS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    )?;
    let mut server_dn = DistinguishedName::new();
    server_dn.push(DnType::CommonName, "localhost");
    server_params.distinguished_name = server_dn;
    server_params.is_ca = IsCa::NoCa;
    server_params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    server_params.not_before = now;
    server_params.not_after = now + time::Duration::days(1825);
    let server_cert = server_params.signed_by(&server_key, &ca_cert, &ca_key)?;

    let server_pem = server_cert.pem();
    let ca_pem = ca_cert.pem();
    fs::write(&*CERT_FILE, format!("{}{}", server_pem, ca_pem))?;
    fs::write(&*KEY_FILE, server_key.serialize_pem())?;
    fs::write(CERT_DIR.join("ca.pem"), ca_pem)?;
    fs::write(CERT_DIR.join("ca.key"), ca_key.serialize_pem())?;
    info!("CA cert saved to {:?}", CERT_DIR.join("ca.pem"));
    Ok(())
}

pub async fn run_connect_server(proxy_port: u16) {
    if *CONNECT_PORT == 0 {
        info!("CONNECT tunnel DISABLED (CONNECT_PORT=0)");
        return;
    }
    ensure_certs();
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", *CONNECT_PORT).parse().unwrap();
    let listener = TcpListener::bind(addr).await.expect("Failed to bind CONNECT server");
    info!("CONNECT+TLS tunnel server listening on 127.0.0.1:{}", *CONNECT_PORT);

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                tokio::spawn(async move { handle_connect(stream, peer, proxy_port).await; });
            }
            Err(e) => error!("CONNECT accept error: {}", e),
        }
    }
}

async fn handle_connect(mut stream: TcpStream, _peer: std::net::SocketAddr, proxy_port: u16) {
    let mut buf = vec![0u8; 4096];
    let n = match tokio::time::timeout(std::time::Duration::from_secs(10), stream.read(&mut buf)).await {
        Ok(Ok(n)) => n,
        _ => return,
    };
    let first_line = String::from_utf8_lossy(&buf[..n]);
    if !first_line.starts_with("CONNECT ") {
        let _ = stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n").await;
        return;
    }
    // Find end of headers
    let header_end = first_line.find("\r\n\r\n").unwrap_or(first_line.len());
    let remaining = if header_end + 4 < n { Some(&buf[header_end+4..n]) } else { None };
    let _ = stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await;
    // Load TLS config
    let cert_pem = fs::read_to_string(&*CERT_FILE).unwrap_or_default();
    let key_pem = fs::read_to_string(&*KEY_FILE).unwrap_or_default();
    let certs = rustls_pemfile::certs(&mut cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    let key = rustls_pemfile::private_key(&mut key_pem.as_bytes()).ok().flatten();

    if certs.is_empty() || key.is_none() {
        error!("Failed to load TLS cert/key");
        return;
    }

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key.unwrap())
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));

    match acceptor.accept(stream).await {
        Ok(mut tls_stream) => {
            let backend_addr = format!("127.0.0.1:{}", proxy_port);
            match TcpStream::connect(&backend_addr).await {
                Ok(mut backend) => {
            if let Some(data) = remaining {
                let _ = backend.write_all(data).await;
            }
                    let (mut brx, mut bwx) = tokio::io::split(backend);
                    let (mut trx, mut twx) = tokio::io::split(tls_stream);
                    let c2b = tokio::io::copy(&mut trx, &mut bwx);
                    let b2c = tokio::io::copy(&mut brx, &mut twx);
                    let _ = tokio::join!(c2b, b2c);
                }
                Err(e) => debug!("CONNECT backend connect error: {}", e),
            }
        }
        Err(e) => warn!("TLS handshake failed: {}", e),
    }
}
