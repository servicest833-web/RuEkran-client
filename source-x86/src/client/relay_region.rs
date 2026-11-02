// Region selection affects outgoing relay negotiation, never registration or direct paths.
use hbb_common::{log, socket_client::connect_tcp, tokio, rendezvous_proto::{RendezvousMessage, RequestRelay}, ResultType, Stream};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
        let ttl = if endpoint.is_some() { 60 } else { 3 };
        if at.elapsed() < Duration::from_secs(ttl) { return endpoint; }
    }
    // Measure verified payload delivery through a private temporary relay pair.
    // Connect/proxy setup is excluded; this measures latency, not bandwidth.
    let (tomsk, moscow) = tokio::join!(probe(TOMSK), probe(MOSCOW));
    let previous = cache.as_ref().and_then(|(_, endpoint)| *endpoint).or_else(last_working);
    let selected = choose_stable(previous, tomsk, moscow);
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
    // A failed temporary pair is retried with fresh sockets AND a fresh UUID.
    // Never append delayed packets from a failed pair to a new measurement.
    for attempt in 0..2 {
        match tokio::time::timeout(Duration::from_millis(4500), probe_delivery(endpoint)).await {
            Ok(Ok(result)) if result.successes >= 3 => return result,
            failure => log::debug!("RuEkran relay probe attempt {} unavailable: {} {:?}", attempt + 1, endpoint, failure),
        }
        if attempt == 0 { tokio::time::sleep(Duration::from_millis(150)).await; }
    }
    Probe { median: None, successes: 0 }
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
    // A successful TCP write is not an acknowledgement that hbbr has paired
    // both requests. Let the pair settle before the unmeasured warm-up packet.
    tokio::time::sleep(Duration::from_millis(100)).await;
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
        (true, true) => choose(tomsk.median, moscow.median),
        (false, false) => None,
    }
}

fn choose_stable(previous: Option<&'static str>, tomsk: Probe, moscow: Probe) -> Option<&'static str> {
    let fastest = choose_probes(tomsk, moscow);
    if let (Some(previous), Some(fastest)) = (previous, fastest) {
        if previous != fastest && tomsk.successes >= 3 && moscow.successes >= 3 {
            if let (Some(t), Some(m)) = (tomsk.median, moscow.median) {
                let (old, new) = if previous == TOMSK { (t, m) } else { (m, t) };
                // Require both a meaningful absolute and relative improvement.
                if old.saturating_sub(new) < 15.max(old / 5) { return Some(previous); }
            }
        }
    }
    fastest
}

fn now_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn saved_region(value: &str, now: u64) -> Option<&'static str> {
    let mut parts = value.split('|');
    let endpoint = region_address(parts.next()?)?;
    let at = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() || at > now || now - at > 86400 { return None; }
    Some(endpoint)
}

fn last_working() -> Option<&'static str> {
    saved_region(&hbb_common::config::LocalConfig::get_option("ruekran-last-working-relay"), now_seconds())
}

fn known_region(endpoint: &str) -> Option<&'static str> {
    match endpoint.split(':').next().unwrap_or("") {
        "78.140.33.193" => Some(TOMSK),
        "msk1-relay.ruekran.ru" => Some(MOSCOW),
        _ => None,
    }
}

/// Only our own relay pool participates; explicit external servers stay intact.
pub(super) fn fallback(endpoint: &str, server_route: &str) -> Option<String> {
    if manual().is_none() && !automatic_allowed(
        &hbb_common::config::Config::get_option("custom-rendezvous-server"),
        &hbb_common::config::Config::get_option("relay-server"),
    ) { return None; }
    fallback_address(endpoint, server_route).map(str::to_owned)
}

fn fallback_address(endpoint: &str, server_route: &str) -> Option<&'static str> {
    let primary = known_region(endpoint)?;
    if let Some(server) = known_region(server_route) {
        if server != primary { return Some(server); }
    }
    Some(if primary == TOMSK { MOSCOW } else { TOMSK })
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
        assert_eq!(choose_probes(Probe { median: Some(5), successes: 1 }, Probe { median: Some(70), successes: 2 }), None);
        assert_eq!(score(&mut []), None);
    }
    #[test]
    fn stable_routes_switch_for_a_real_gain_or_confirmed_failure() {
        let p = |median, successes| Probe { median, successes };
        assert_eq!(choose_stable(Some(TOMSK), p(Some(65), 5), p(Some(63), 5)), Some(TOMSK));
        assert_eq!(choose_stable(Some(MOSCOW), p(Some(0), 5), p(Some(67), 5)), Some(TOMSK));
        assert_eq!(choose_stable(Some(TOMSK), p(None, 0), p(Some(67), 5)), Some(MOSCOW));
        assert_eq!(choose_stable(Some(TOMSK), p(Some(0), 1), p(Some(67), 1)), None);
        assert_eq!(choose_stable(Some(MOSCOW), p(Some(90), 5), p(Some(100), 5)), Some(MOSCOW));
        assert_eq!(choose_stable(Some(MOSCOW), p(Some(70), 5), p(Some(100), 5)), Some(TOMSK));
    }
    #[test]
    fn saved_hints_are_bounded_and_fallback_is_always_a_different_known_region() {
        assert_eq!(saved_region("tomsk|100", 200), Some(TOMSK));
        assert_eq!(saved_region("tomsk|100", 86501), None);
        assert_eq!(saved_region("moscow|201", 200), None);
        assert_eq!(saved_region("other|100", 200), None);
        assert_eq!(saved_region("tomsk|100|extra", 200), None);
        assert_eq!(fallback_address(TOMSK, TOMSK), Some(MOSCOW));
        assert_eq!(fallback_address(MOSCOW, TOMSK), Some(TOMSK));
        assert_eq!(fallback_address("custom.example:21117", TOMSK), None);
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
    // Called only after the peer's secure handshake, never after a probe alone.
    if let Some(region) = known_region(endpoint) {
        let value = format!("{}|{}", if region == TOMSK { "tomsk" } else { "moscow" }, now_seconds());
        hbb_common::config::LocalConfig::set_option("ruekran-last-working-relay".to_owned(), value);
        if let Ok(mut cache) = CACHE.try_lock() {
            *cache = Some((Instant::now(), Some(region)));
        }
    }
}
