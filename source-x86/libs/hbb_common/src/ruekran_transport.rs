//! Fixed, authenticated server ingress for clients behind a configured proxy.
//! This is transport encryption; peer authentication and desktop encryption remain separate.
use crate::{config::Socks5Server, tcp::{DynTcpStream, FramedStream}, ResultType, Stream};
use anyhow::{bail, Context};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_native_tls::native_tls;
use tokio_rustls::{rustls, TlsConnector};
use rustls_pki_types::{CertificateDer, ServerName};
use std::{convert::TryFrom, sync::Arc};

#[cfg(windows)]
#[path = "ruekran_p521.rs"]
mod p521;

pub const RT_CA_FINGERPRINT: &str = "6b8b9095712c215725210115df3f65da6e4b7e835779b81c473c30cb7cd86da8";
static RT_CA_REQUIRED_PROXY: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn rt_ca_required() -> bool {
    let required = RT_CA_REQUIRED_PROXY.lock().unwrap();
    !required.is_empty() && crate::config::Config::get_socks().map(|c| c.proxy == *required).unwrap_or(false)
}

pub fn certificate_der(bytes: &[u8]) -> ResultType<Vec<u8>> {
    if bytes.is_empty() || bytes.len() > 65536 { bail!("Certificate file must contain 1–65536 bytes"); }
    let cert = if bytes.starts_with(b"-----BEGIN") {
        native_tls::Certificate::from_pem(bytes)?
    } else { native_tls::Certificate::from_der(bytes)? };
    let der = cert.to_der()?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(CertificateDer::from(der.clone()))?;
    Ok(der)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: &'static str,
    pub path: &'static str,
}

/// Exact owned hosts and native ports only: user-specified external servers keep their route.
pub fn endpoint(target: &str, registration: bool) -> Option<Endpoint> {
    let (host, port) = target.rsplit_once(':')?;
    let tomsk = ["adm.stilservice70.ru", "ruekran.ru", "78.140.33.193", "10.0.0.28"]
        .contains(&host);
    if registration {
        return (tomsk && port == "21116").then_some(Endpoint {
            host: "ruekran.ru", path: "/connect/v1/host-id",
        });
    }
    if tomsk {
        let path = match port {
            "21115" => "/connect/v1/presence",
            "21116" => "/connect/v1/id",
            "21117" => "/connect/v1/relay",
            _ => return None,
        };
        return Some(Endpoint { host: "ruekran.ru", path });
    }
    if ["msk1-relay.ruekran.ru", "2.153.248.148"].contains(&host) && port == "21117" {
        return Some(Endpoint { host: "msk1-relay.ruekran.ru", path: "/relay" });
    }
    None
}

pub fn system_proxy_url(value: &str) -> Option<String> {
    let value = value.trim();
    let (scheme, address) = if value.contains('=') {
        let entries: Vec<_> = value.split(';').filter_map(|v| v.trim().split_once('=')).collect();
        let mut selected = None;
        for key in ["https", "http", "socks"] {
            if let Some((_, value)) = entries.iter().find(|(name, _)| name.eq_ignore_ascii_case(key)) {
                selected = Some((if key == "socks" { "socks5" } else { "http" }, value.trim()));
                break;
            }
        }
        selected?
    } else { ("http", value) };
    let address = if address.contains("://") { address.to_owned() } else { format!("{scheme}://{address}") };
    let parsed = url::Url::parse(&address).ok()?;
    if !["http", "https", "socks5"].contains(&parsed.scheme()) || parsed.host_str().is_none()
        || parsed.query().is_some() || parsed.fragment().is_some() || parsed.path() != "/" && !parsed.path().is_empty() {
        return None;
    }
    Some(address)
}

fn verify_upgrade(bytes: &[u8]) -> ResultType<()> {
    let text = std::str::from_utf8(bytes)?;
    let mut lines = text.strip_suffix("\r\n\r\n").context("Incomplete TLS Upgrade response")?.split("\r\n");
    let status = lines.next().unwrap_or_default();
    if status != "HTTP/1.1 101" && !status.starts_with("HTTP/1.1 101 ") {
        bail!("Server refused the authenticated TLS Upgrade");
    }
    let mut headers = std::collections::HashMap::new();
    for line in lines {
        let (key, value) = line.split_once(':').context("Invalid TLS Upgrade header")?;
        if key.is_empty() || headers.insert(key.to_ascii_lowercase(), value.trim()).is_some() {
            bail!("Ambiguous TLS Upgrade headers");
        }
    }
    if !headers.get("upgrade").map(|s| s.eq_ignore_ascii_case("ruekran-v1")).unwrap_or(false)
        || !headers.get("connection").map(|s| s.split(',').any(|v| v.trim().eq_ignore_ascii_case("upgrade"))).unwrap_or(false)
        || headers.contains_key("content-length") || headers.contains_key("transfer-encoding") {
        bail!("Server did not authenticate the expected Upgrade protocol");
    }
    Ok(())
}

/// No direct/plaintext retry after a TLS, certificate, proxy or Upgrade failure.
pub async fn open(endpoint: Endpoint, conf: &Socks5Server, timeout: u64) -> ResultType<Stream> {
    let approved = crate::config::Config::get_option("ruekran-rt-ca-approved") == RT_CA_FINGERPRINT;
    let rt_certificate = || certificate_der(include_bytes!("ruekran_espd_rt.pem"));
    if approved && endpoint.host == "ruekran.ru" {
        let result = open_with_ca(endpoint, conf, timeout, Some(rt_certificate()?)).await;
        if result.is_ok() { RT_CA_REQUIRED_PROXY.lock().unwrap().clear(); }
        return result;
    }
    let original = open_with_ca(endpoint, conf, timeout, None).await;
    match original {
        Ok(stream) => {
            if endpoint.host == "ruekran.ru" { RT_CA_REQUIRED_PROXY.lock().unwrap().clear(); }
            Ok(stream)
        }
        Err(err) => {
            // Probe only certificate issuer/algorithm failures on our fixed TLS ingress.
            // The candidate must validate name, dates and signatures against the
            // known CA. It is dropped before sending an ID or any protocol payload.
            let certificate_error = format!("{err:#}");
            if endpoint.host == "ruekran.ru" && (certificate_error.contains("UnknownIssuer")
                || certificate_error.contains("UnsupportedSignatureAlgorithm")) {
                match open_with_ca(endpoint, conf, timeout, Some(rt_certificate()?)).await {
                    Ok(candidate) => {
                        drop(candidate);
                        *RT_CA_REQUIRED_PROXY.lock().unwrap() = conf.proxy.clone();
                        bail!("Certificate approval required for the Rostelecom proxy network");
                    }
                    Err(candidate_error) => {
                        return Err(err.context(format!("Known Rostelecom CA probe failed: {candidate_error:#}")));
                    }
                }
            }
            Err(err)
        }
    }
}

pub async fn open_with_ca(endpoint: Endpoint, conf: &Socks5Server, timeout: u64, extra: Option<Vec<u8>>) -> ResultType<Stream> {
    crate::timeout(timeout, async {
        let target = format!("{}:443", endpoint.host);
        let raw = FramedStream::connect(target.as_str(), None, conf, timeout).await
            .context("Connecting to the configured proxy / opening CONNECT tunnel")?;
        let address = raw.local_addr();
        // Preserve the CONNECT parser's buffered bytes by moving its entire stream.
        let raw = raw.0.into_inner();
        let mut tls = if endpoint.host == "msk1-relay.ruekran.ru" {
            // This existing self-signed service certificate is marked CA=true;
            // WebPKI refuses it as an end entity. Keep strict native name/date
            // validation with this sole explicitly pinned trust anchor.
            let mut builder = native_tls::TlsConnector::builder();
            builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
            builder.disable_built_in_roots(true);
            builder.add_root_certificate(native_tls::Certificate::from_pem(include_bytes!("ruekran_moscow.pem"))?);
            let connector = tokio_native_tls::TlsConnector::from(builder.build()?);
            DynTcpStream(Box::new(connector.connect(endpoint.host, raw).await.context("Validating the Moscow service certificate")?))
        } else {
            // WebPKI validates chain, name, dates and handshake signatures without
            // Schannel's mandatory online revocation request, unavailable behind ESPD.
            #[cfg(windows)]
            let config = p521::client_config(extra.map(CertificateDer::from))?;
            #[cfg(not(windows))]
            let config = crate::verifier::client_config_safe_with_extra_root(extra.map(CertificateDer::from))?;
            let connector = TlsConnector::from(Arc::new(config));
            let name = ServerName::try_from(endpoint.host)?.to_owned();
            DynTcpStream(Box::new(connector.connect(name, raw).await.context("Validating the server TLS certificate")?))
        };
        if endpoint.host == "ruekran.ru" {
            use futures::{SinkExt, StreamExt};
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
            let path = endpoint.path.replace("/connect/v1/", "/connect/ws/");
            let request = format!("wss://{}{path}", endpoint.host).into_client_request()?;
            let mut config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
            config.max_message_size = Some(65536);
            config.max_frame_size = Some(65536);
            let (socket, _) = tokio_tungstenite::client_async_with_config(request, tls, Some(config)).await
                .context("Opening the standard secure WebSocket transport")?;
            let (application, bridge) = tokio::io::duplex(65536);
            tokio::spawn(async move {
                let (mut input, mut output) = tokio::io::split(bridge);
                let (mut sender, mut receiver) = socket.split();
                let incoming = async {
                    while let Some(message) = receiver.next().await {
                        match message? {
                            Message::Binary(bytes) => output.write_all(&bytes).await?,
                            Message::Close(_) => break,
                            Message::Ping(_) | Message::Pong(_) => (),
                            _ => anyhow::bail!("Unexpected WebSocket message type"),
                        }
                    }
                    anyhow::Ok(())
                };
                let outgoing = async {
                    let mut buffer = [0; 65536];
                    loop {
                        let length = input.read(&mut buffer).await?;
                        if length == 0 { break; }
                        sender.send(Message::Binary(buffer[..length].to_vec().into())).await?;
                    }
                    anyhow::Ok(())
                };
                tokio::select! { _ = incoming => (), _ = outgoing => () }
            });
            let mut framed = FramedStream::from(DynTcpStream(Box::new(application)), address);
            framed.4 = true;
            return Ok(Stream::Tcp(framed));
        }
        let request = format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: Upgrade\r\nUpgrade: ruekran-v1\r\n\r\n", endpoint.path, endpoint.host);
        tls.write_all(request.as_bytes()).await?;
        tls.flush().await?;
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            if header.len() >= 8192 { bail!("Oversized TLS Upgrade response"); }
            header.push(tls.read_u8().await.context("Reading the TLS Upgrade response")?);
        }
        verify_upgrade(&header)?;
        let mut framed = FramedStream::from(tls, address);
        framed.4 = true;
        log::info!("Authenticated proxy transport: {}{}", endpoint.host, endpoint.path);
        Ok(Stream::Tcp(framed))
    }).await.context("Authenticated proxy transport timed out")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_proxy_maps_https_destinations_to_http_connect() {
        assert_eq!(system_proxy_url("10.0.70.52:3128").as_deref(), Some("http://10.0.70.52:3128"));
        assert_eq!(system_proxy_url("http=10.0.70.52:3128;https=10.0.54.52:3128").as_deref(), Some("http://10.0.54.52:3128"));
        assert_eq!(system_proxy_url("socks=localhost:1080").as_deref(), Some("socks5://localhost:1080"));
        assert!(system_proxy_url("file:///bad").is_none());
        assert!(system_proxy_url("").is_none());
    }
    #[test]
    fn fixed_routes_preserve_external_servers() {
        assert_eq!(endpoint("adm.stilservice70.ru:21116", true).unwrap().path, "/connect/v1/host-id");
        assert_eq!(endpoint("78.140.33.193:21117", false).unwrap().path, "/connect/v1/relay");
        assert_eq!(endpoint("msk1-relay.ruekran.ru:21117", false).unwrap().host, "msk1-relay.ruekran.ru");
        for target in ["example.org:21116", "ruekran.ru.example.org:21116", "ruekran.ru:443", "msk1-relay.ruekran.ru:21116"] {
            assert!(endpoint(target, false).is_none());
        }
        assert!(endpoint("ruekran.ru:21117", true).is_none());
    }
    #[test]
    fn upgrade_rejects_ambiguous_or_downgraded_responses() {
        let good = b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: ruekran-v1\r\n\r\n";
        assert!(verify_upgrade(good).is_ok());
        for bad in ["HTTP/1.1 200 OK\r\n\r\n", "HTTP/1.1 101 OK\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n", "HTTP/1.1 101 OK\r\nConnection: Upgrade\r\nUpgrade: ruekran-v1\r\nUpgrade: ruekran-v1\r\n\r\n", "HTTP/1.1 101 OK\r\nConnection: Upgrade\r\nUpgrade: ruekran-v1\r\nContent-Length: 0\r\n\r\n"] {
            assert!(verify_upgrade(bad.as_bytes()).is_err());
        }
    }
}
