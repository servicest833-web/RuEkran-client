// Region selection affects outgoing relay negotiation, never registration or direct paths.
use hbb_common::{log, socket_client::connect_tcp, tokio, rendezvous_proto::{RendezvousMessage, RequestRelay}, ResultType, Stream};
use std::time::{Duration, Instant};

const TOMSK: &str = "78.140.33.193:21117";
const MOSCOW: &str = "msk1-relay.ruekran.ru:21117";
lazy_static::lazy_static! {
    static ref CACHE: tokio::sync::Mutex<Option<(Instant, Option<&'static str>)>> =
        tokio::sync::Mutex::new(None);
}

pub(super) fn manual() -> Option<&'static str> {
    region_address(&hbb_common::config::LocalConfig::get_option("ruekran-test-relay"))
}

pub(super) async fn selected() -> Option<&'static str> {
    if let Some(endpoint) = manual() {
        if probe(endpoint).await.successes >= 3 { return Some(endpoint); }
        let fallback = if endpoint == MOSCOW { TOMSK } else { MOSCOW };
        let available = probe(fallback).await.successes >= 3;
        log::warn!("RuEkran preferred relay unavailable: {}; fallback={} available={}", endpoint, fallback, available);
        return if available { Some(fallback) } else { None };
    }
    if !automatic_allowed(
        &hbb_common::config::Config::get_option("custom-rendezvous-server"),
        &hbb_common::config::Config::get_option("relay-server"),
    ) { return None; }
    let mut cache = CACHE.lock().await;
    if let Some((at, endpoint)) = *cache {
        if at.elapsed() < Duration::from_secs(10) { return endpoint; }
    }
    // Measure verified payload delivery through a private temporary relay pair.
    // Connect/proxy setup is excluded; this measures latency, not bandwidth.
    let (tomsk, moscow) = tokio::join!(probe(TOMSK), probe(MOSCOW));
    let selected = choose_probes(tomsk, moscow);
    log::info!("RuEkran relay probe: TSK={:?}ms MSK1={:?}ms selected={:?}", tomsk, moscow, selected);
    *cache = Some((Instant::now(), selected));
    selected
}

fn automatic_allowed(id_server: &str, relay_server: &str) -> bool {
    // Explicit third-party server settings must remain authoritative.
    [id_server, relay_server].iter().all(|server| {
        let host = server.split(':').next().unwrap_or("").trim();
        host.is_empty() || ["adm.stilservice70.ru", "ruekran.ru", "78.140.33.193",
            "10.0.0.28", "msk1-relay.ruekran.ru", "2.153.248.148"].contains(&host)
    })
}

#[derive(Clone, Copy, Debug)]
struct Probe { median: Option<u64>, successes: usize }

async fn probe(endpoint: &'static str) -> Probe {
    match tokio::time::timeout(Duration::from_secs(10), probe_delivery(endpoint)).await {
        Ok(Ok(result)) => result,
        failure => { log::debug!("RuEkran relay delivery probe unavailable: {} {:?}", endpoint, failure); Probe { median: None, successes: 0 } }
    }
}

async fn probe_delivery(endpoint: &str) -> ResultType<Probe> {
    let mut sender = connect_tcp(endpoint, 1500).await?;
    let mut receiver = connect_tcp(endpoint, 1500).await?;
    sender.set_max_packet_length(1024);
    receiver.set_max_packet_length(1024);
    sender.set_send_timeout(1000);
    receiver.set_send_timeout(1000);
    let uuid = uuid::Uuid::new_v4().to_string();
    let configured_key = hbb_common::config::Config::get_option("key");
    let mut request = RendezvousMessage::new();
    request.set_request_relay(RequestRelay {
        uuid: uuid.clone(),
        licence_key: if configured_key.is_empty() { hbb_common::config::RS_PUB_KEY.to_owned() } else { configured_key },
        ..Default::default()
    });
    sender.send(&request).await?;
    receiver.send(&request).await?;
    let mut payload = uuid.into_bytes();
    // The first relay-forwarded packet warms the pair; no remote computer is involved.
    exchange(&mut sender, &mut receiver, &payload).await?;
    tokio::time::sleep(Duration::from_millis(80)).await;
    let mut samples = Vec::new();
    for index in 0..5 {
        payload[0] = index;
        match exchange(&mut sender, &mut receiver, &payload).await {
            Ok(elapsed) => samples.push(elapsed),
            Err(_) => break,
        }
        if index < 4 { tokio::time::sleep(Duration::from_millis(100)).await; }
    }
    log::info!("RuEkran relay delivery {} samples={:?}ms", endpoint, samples);
    Ok(Probe { successes: samples.len(), median: score(&mut samples) })
}

/// Standalone audit entry: no registration, login, remote input or preference writes.
pub(super) async fn audit() -> ResultType<()> {
    for endpoint in [TOMSK, MOSCOW] {
        let mut tcp = Vec::new();
        for _ in 0..3 {
            let start = Instant::now();
            if connect_tcp(endpoint, 1000).await.is_ok() { tcp.push(start.elapsed().as_millis() as u64); }
        }
        let delivered = probe(endpoint).await;
        println!("{} connect_only={:?}ms relay_delivery={:?}ms successful={}/5", endpoint, score(&mut tcp), delivered.median, delivered.successes);
    }
    let (tomsk, moscow) = tokio::join!(probe(TOMSK), probe(MOSCOW));
    println!("Fresh automatic selection: {:?}; TSK={:?}, MSK={:?}", choose_probes(tomsk, moscow), tomsk, moscow);
    if tomsk.median.is_none() && moscow.median.is_none() { hbb_common::bail!("Neither relay answered the forwarded-data check"); }
    Ok(())
}

async fn exchange(sender: &mut Stream, receiver: &mut Stream, payload: &[u8]) -> ResultType<u64> {
    let start = Instant::now();
    sender.send_raw(payload.to_vec()).await?;
    let reply = receiver.next_timeout(1500).await.ok_or_else(|| hbb_common::anyhow::anyhow!("Relay delivery timeout"))??;
    if reply.as_ref() != payload { hbb_common::bail!("Relay probe response mismatch"); }
    Ok(start.elapsed().as_millis() as u64)
}

fn score(samples: &mut [u64]) -> Option<u64> {
    if samples.is_empty() { return None; }
    samples.sort_unstable();
    Some(samples[samples.len() / 2])
}

fn choose_probes(tomsk: Probe, moscow: Probe) -> Option<&'static str> {
    match (tomsk.successes >= 3, moscow.successes >= 3) {
        (true, false) => Some(TOMSK),
        (false, true) => Some(MOSCOW),
        _ => choose(tomsk.median, moscow.median),
    }
}

fn choose(tomsk: Option<u64>, moscow: Option<u64>) -> Option<&'static str> {
    match (tomsk, moscow) {
        (Some(t), Some(m)) if m < t => Some(MOSCOW),
        (Some(_), _) => Some(TOMSK),
        (_, Some(_)) => Some(MOSCOW),
        _ => None, // Preserve the server-provided fallback if neither probe succeeded.
    }
}

fn region_address(value: &str) -> Option<&'static str> {
    match value {
        "tomsk" => Some(TOMSK),
        "moscow" => Some(MOSCOW),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_external_servers_are_not_overridden() {
        assert!(automatic_allowed("", ""));
        assert!(automatic_allowed("adm.stilservice70.ru:21116", "78.140.33.193:21117"));
        assert!(!automatic_allowed("other.example", ""));
        assert!(!automatic_allowed("", "custom-relay.example:21117"));
    }
    #[test]
    fn measured_latency_and_failures_determine_the_route() {
        assert_eq!(choose(Some(90), Some(20)), Some(MOSCOW));
        assert_eq!(choose(Some(10), Some(80)), Some(TOMSK));
        assert_eq!(choose(None, Some(20)), Some(MOSCOW));
        assert_eq!(choose(Some(20), None), Some(TOMSK));
        assert_eq!(choose(None, None), None);
        assert_eq!(score(&mut [80, 20, 25]), Some(25));
        assert_eq!(score(&mut [1]), Some(1));
        assert_eq!(choose_probes(Probe { median: Some(5), successes: 4 }, Probe { median: Some(70), successes: 5 }), Some(TOMSK));
        assert_eq!(choose_probes(Probe { median: Some(5), successes: 1 }, Probe { median: Some(70), successes: 5 }), Some(MOSCOW));
        assert_eq!(score(&mut []), None);
    }
    #[test]
    fn only_known_regions_override_the_server() {
        assert_eq!(region_address("moscow"), Some("msk1-relay.ruekran.ru:21117"));
        assert_eq!(region_address("tomsk"), Some("78.140.33.193:21117"));
        assert_eq!(region_address(""), None);
        assert_eq!(region_address("untrusted.example"), None);
    }
}

pub(super) fn record(interface: &impl super::Interface, direct: bool, transport: &str, endpoint: &str) {
    let endpoint = if direct || transport.starts_with("WebRTC") { "" } else { endpoint };
    interface.get_lch().write().unwrap().ruekran_relay_endpoint = endpoint.to_owned();
}
