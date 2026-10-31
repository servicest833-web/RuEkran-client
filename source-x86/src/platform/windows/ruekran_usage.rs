use hbb_common::{config::Config, log, serde_json::json, sodiumoxide::crypto::sign};
use std::{collections::HashMap, sync::{Mutex, Once}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

struct Entry {
    sid: String,
    correlation: String,
    peer: String,
    direction: &'static str,
    started: u64,
    clock: Instant,
    closed: Option<u64>,
    sequence: u64,
    route: &'static str,
    relay: String,
}

lazy_static::lazy_static! {
    static ref ENTRIES: Mutex<HashMap<String, Entry>> = Mutex::new(HashMap::new());
}
static WORKER: Once = Once::new();

fn enabled() -> bool {
    true
}

fn timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|v| v.as_secs()).unwrap_or(0)
}

pub struct Scope(String);

pub fn scope(key: String) -> Scope {
    Scope(key)
}

pub fn outgoing_key(peer: String, session: u64) -> String {
    format!("outgoing:{}:{}", peer, session)
}

impl Drop for Scope {
    fn drop(&mut self) {
        finish(&self.0);
    }
}

pub fn activate(key: String, peer: String, direction: &'static str, session: u64) {
    if !enabled() { return; }
    let peer = if !peer.is_empty() && peer.len() <= 16 && peer.bytes().all(|v| v.is_ascii_digit()) { peer } else { "direct-or-external".to_owned() };
    let own = Config::get_id();
    let (source, target) = if direction == "outgoing" { (&own, &peer) } else { (&peer, &own) };
    let correlation = if session != 0 && peer != "direct-or-external" {
        let value = format!("ruekran-usage-link-v1:{}:{}:{}", source, target, session);
        hbb_common::sodiumoxide::crypto::hash::sha256::hash(value.as_bytes()).0.iter().map(|v| format!("{:02x}", v)).collect()
    } else { String::new() };
    let mut entries = ENTRIES.lock().unwrap();
    if entries.len() >= 32 && !entries.contains_key(&key) {
        let oldest = entries.iter().filter(|(_, entry)| entry.closed.is_some())
            .min_by_key(|(_, entry)| entry.started).map(|(key, _)| key.clone());
        if let Some(oldest) = oldest {
            entries.remove(&oldest);
            hbb_common::throttled_log!(Duration::from_secs(60), warn, "Session statistics queue full; oldest completed report discarded");
        } else {
            hbb_common::throttled_log!(Duration::from_secs(60), warn, "Session statistics active capacity reached");
            return;
        }
    }
    if entries.get(&key).map(|entry| entry.closed.is_some()).unwrap_or(false) { entries.remove(&key); }
    entries.entry(key).or_insert_with(|| Entry {
        sid: uuid::Uuid::new_v4().to_string(), correlation, peer, direction,
        started: timestamp(), clock: Instant::now(), closed: None, sequence: 0, route: "unknown", relay: String::new(),
    });
    drop(entries);
    WORKER.call_once(|| { std::thread::spawn(report_loop); });
}

pub fn finish(key: &str) {
    if let Some(entry) = ENTRIES.lock().unwrap().get_mut(key) {
        entry.closed = Some(entry.clock.elapsed().as_secs());
    }
}

pub fn set_route(key: &str, direct: Option<bool>, endpoint: String) {
    if let Some(entry) = ENTRIES.lock().unwrap().get_mut(key) {
        match direct {
            Some(true) => { entry.route = "direct"; entry.relay.clear(); }
            Some(false) if !endpoint.is_empty() => { entry.route = "relay"; entry.relay = endpoint; }
            _ => {}
        }
    }
}

fn report_loop() {
    let client = match reqwest::blocking::Client::builder()
        .use_rustls_tls().https_only(true).redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(4)).build() {
        Ok(client) => client,
        Err(error) => { log::warn!("Session statistics unavailable: {}", error); return; }
    };
    loop {
        if !enabled() {
            ENTRIES.lock().unwrap().clear();
        } else {
            let (sessions, closed) = {
                let mut entries = ENTRIES.lock().unwrap();
                let mut sessions = Vec::new();
                let mut closed = Vec::new();
                for (key, entry) in entries.iter_mut().take(32) {
                    entry.sequence += 1;
                    sessions.push(json!({"sid":entry.sid,"correlation":entry.correlation,"peer":entry.peer,"direction":entry.direction,
                        "started":entry.started,"elapsed":entry.closed.unwrap_or_else(|| entry.clock.elapsed().as_secs()),
                        "closed":entry.closed.is_some(),"sequence":entry.sequence,"route":entry.route,"relay":entry.relay}));
                    if entry.closed.is_some() { closed.push((key.clone(), entry.sid.clone())); }
                }
                (sessions, closed)
            };
            if !sessions.is_empty() {
                let body = json!({"purpose":"ruekran-session-report-v1","id":Config::get_id(),
                    "timestamp":timestamp(),"nonce":uuid::Uuid::new_v4().to_string(),"sessions":sessions}).to_string();
                if let Some(secret) = sign::SecretKey::from_slice(&Config::get_key_pair().0) {
                    let envelope = json!({"payload":crate::encode64(body.as_bytes()),
                        "signature":crate::encode64(sign::sign_detached(body.as_bytes(), &secret).to_bytes())});
                    match client.post("https://ruekran.ru/client/session-report").json(&envelope).send() {
                        Ok(response) if response.status().is_success() => {
                            let mut entries = ENTRIES.lock().unwrap();
                            for (key, sid) in closed {
                                if entries.get(&key).map(|entry| entry.sid == sid).unwrap_or(false) { entries.remove(&key); }
                            }
                        }
                        _ => hbb_common::throttled_log!(Duration::from_secs(60), warn, "Session statistics report was not acknowledged"),
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}
