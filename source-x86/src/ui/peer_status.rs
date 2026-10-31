use hbb_common::{
    anyhow::bail,
    config::Config,
    log,
    rendezvous_proto::{rendezvous_message, OnlineRequest, RendezvousMessage},
    socket_client::connect_tcp,
    tokio::{self, sync::mpsc, time::timeout},
    ResultType,
};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

lazy_static::lazy_static! {
    static ref REQUEST: Mutex<Option<mpsc::Sender<Vec<String>>>> = Mutex::new(None);
    static ref SNAPSHOT: Mutex<Option<(Instant, String)>> = Mutex::new(None);
}

// Run on the existing connection-status runtime, never on Sciter's UI thread.
pub fn start() {
    let (tx, mut rx) = mpsc::channel::<Vec<String>>(1);
    *REQUEST.lock().unwrap() = Some(tx);
    tokio::spawn(async move {
        while let Some(ids) = rx.recv().await {
            let result = timeout(Duration::from_secs(8), async {
                let (server, _, _) = crate::get_rendezvous_server(1_000).await;
                query(&server, &ids).await
            })
            .await;
            let states = match result {
                Ok(Ok(states)) => states,
                other => {
                    log::trace!("Peer availability query failed: {:?}", other);
                    HashMap::new()
                }
            };
            match serde_json::to_string(&states) {
                Ok(json) => *SNAPSHOT.lock().unwrap() = Some((Instant::now(), json)),
                Err(error) => log::trace!("Peer availability serialization failed: {}", error),
            }
        }
    });
}

pub fn request(ids: Vec<String>) {
    let mut ids = ids;
    ids.retain(|id| {
        !id.is_empty() && id.len() <= 100 &&
            id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    });
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return;
    }
    if let Some(sender) = REQUEST.lock().unwrap().as_ref() {
        // A bounded queue prevents repeated UI refreshes from accumulating work.
        if let Err(error) = sender.try_send(ids) {
            log::trace!("Peer availability request not queued: {}", error);
        }
    }
}

pub fn snapshot() -> String {
    match SNAPSHOT.lock().unwrap().as_ref() {
        Some((at, json)) if at.elapsed() < Duration::from_secs(30) => json.clone(),
        _ => "{}".to_owned(),
    }
}

async fn query(server: &str, ids: &[String]) -> ResultType<HashMap<String, bool>> {
    let Some((host, port)) = server.rsplit_once(':') else {
        bail!("Invalid rendezvous address");
    };
    let port = port.parse::<u16>()?;
    if host.is_empty() || port <= 1 {
        bail!("Invalid rendezvous port");
    }
    let mut socket = connect_tcp(format!("{}:{}", host, port - 1), 3_000).await?;
    socket.set_max_packet_length(64 * 1024);
    let mut request = RendezvousMessage::new();
    request.set_online_request(OnlineRequest {
        id: Config::get_id(),
        peers: ids.to_vec(),
        ..Default::default()
    });
    socket.send(&request).await?;
    if let Some(response) = crate::get_next_nonkeyexchange_msg(&mut socket, Some(3_000)).await {
        if let Some(rendezvous_message::Union::OnlineResponse(response)) = response.union {
            return decode_states(ids, &response.states);
        }
    }
    bail!("No peer availability response")
}

fn decode_states(ids: &[String], states: &[u8]) -> ResultType<HashMap<String, bool>> {
    if states.len() < (ids.len() + 7) / 8 {
        bail!("Truncated peer availability response");
    }
    Ok(ids.iter().enumerate().map(|(i, id)| {
        (id.clone(), states[i / 8] & (1 << (7 - i % 8)) != 0)
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_online_and_offline_across_byte_boundary() {
        let ids: Vec<String> = (0..9).map(|i| format!("peer{i}")).collect();
        let states = decode_states(&ids, &[0b1000_0001, 0b1000_0000]).unwrap();
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(states[id], i == 0 || i == 7 || i == 8);
        }
    }

    #[test]
    fn truncated_response_is_an_error_not_an_offline_result() {
        assert!(decode_states(&["peer".to_owned()], &[]).is_err());
        assert!(decode_states(&[], &[]).unwrap().is_empty());
    }

    #[tokio::test]
    async fn refused_connection_is_an_error_not_an_offline_result() {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let server = format!("127.0.0.1:{}", port + 1);
        assert!(query(&server, &["peer".to_owned()]).await.is_err());
    }
}
