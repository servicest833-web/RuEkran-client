//! Isolated live transport probe. Never writes local client configuration.
use hbb_common::{config::Socks5Server, protobuf::Message, rendezvous_proto::*, ruekran_transport, Stream};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let conf = Socks5Server { proxy: args.get(1).expect("proxy URL").clone(), ..Default::default() };
    let target = args.get(2).map(String::as_str).unwrap_or("adm.stilservice70.ru:21115");
    let registration = args.get(3).map(String::as_str) == Some("--register");
    let endpoint = ruekran_transport::endpoint(target, registration).expect("owned target");
    let mut stream = if let Some(index) = args.iter().position(|v| v == "--ca") {
        let cert = std::fs::read(args.get(index + 1).context("CA path missing")?)?;
        ruekran_transport::open_with_ca(endpoint, &conf, 10000, Some(ruekran_transport::certificate_der(&cert)?)).await?
    } else { ruekran_transport::open(endpoint, &conf, 10000).await? };
    assert!(stream.is_transport_authenticated());
    assert!(!stream.is_secured(), "TLS must not impersonate peer E2E encryption");
    if registration {
        hbb_common::sodiumoxide::init().map_err(|_| anyhow::anyhow!("crypto init"))?;
        let (pk, _sk) = hbb_common::sodiumoxide::crypto::sign::gen_keypair();
        let id = format!("{}", hbb_common::rand::random::<u32>() as u64 + 10_000_000_000);
        let mut request = RendezvousMessage::new();
        request.set_register_pk(RegisterPk { id: id.clone(), uuid: vec![19;16].into(), pk: pk.0.to_vec().into(), ..Default::default() });
        stream.send(&request).await?;
        let response = stream.next_timeout(5000).await.context("no registration reply")??;
        let reply = RendezvousMessage::parse_from_bytes(&response)?;
        let Some(rendezvous_message::Union::RegisterPkResponse(result)) = reply.union else { anyhow::bail!("wrong registration response"); };
        println!("REGISTER_RESULT {:?}", result.result);
        anyhow::ensure!(result.result.enum_value_or_default() == register_pk_response::Result::OK, "registration rejected");
        for _ in 0..3 {
            tokio::time::sleep(std::time::Duration::from_secs(8)).await;
            request.set_register_peer(RegisterPeer { id: id.clone(), ..Default::default() });
            stream.send(&request).await?;
            let response = stream.next_timeout(5000).await.context("no heartbeat reply")??;
            let reply = RendezvousMessage::parse_from_bytes(&response)?;
            anyhow::ensure!(matches!(reply.union, Some(rendezvous_message::Union::RegisterPeerResponse(_))), "wrong heartbeat reply");
            println!("PASS registration heartbeat");
        }
        let control_endpoint = ruekran_transport::endpoint(target, false).unwrap();
        let mut controller = ruekran_transport::open(control_endpoint, &conf, 10000).await?;
        let mut incoming = RendezvousMessage::new();
        incoming.set_punch_hole_request(PunchHoleRequest {
            id, licence_key: hbb_common::config::RS_PUB_KEY.to_owned(),
            nat_type: NatType::SYMMETRIC.into(), force_relay: true,
            version: "1.5.0".to_owned(), ..Default::default()
        });
        controller.send(&incoming).await?;
        let response = stream.next_timeout(5000).await.context("no incoming connection request")??;
        let reply = RendezvousMessage::parse_from_bytes(&response)?;
        anyhow::ensure!(matches!(reply.union, Some(rendezvous_message::Union::PunchHole(_)) | Some(rendezvous_message::Union::RequestRelay(_)) | Some(rendezvous_message::Union::FetchLocalAddr(_))), "wrong incoming connection request");
        println!("PASS incoming request delivered through the registered proxy stream");
        println!("PASS isolated TLS host registration");
    } else if endpoint.path.ends_with("/presence") || endpoint.path.ends_with("/id") {
        let mut request = RendezvousMessage::new();
        request.set_test_nat_request(TestNatRequest::default());
        stream.send(&request).await?;
        let response = stream.next_timeout(5000).await.context("no diagnostic reply")??;
        let reply = RendezvousMessage::parse_from_bytes(&response)?;
        assert!(matches!(reply.union, Some(rendezvous_message::Union::TestNatResponse(_))));
        println!("PASS authenticated {}{}: diagnostic reply", endpoint.host, endpoint.path);
    } else if args.get(3).map(String::as_str) == Some("--pair") {
        let mut second = ruekran_transport::open(endpoint, &conf, 10000).await?;
        let mut request = RendezvousMessage::new();
        request.set_request_relay(RequestRelay {
            id: "ruekran-http-proxy-qa".to_owned(),
            uuid: format!("proxy-qa-{}", hbb_common::rand::random::<u64>()),
            licence_key: hbb_common::config::RS_PUB_KEY.to_owned(), ..Default::default()
        });
        stream.send(&request).await?;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        second.send(&request).await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        stream.set_raw(); second.set_raw();
        let payload: Vec<u8> = (0..65536).map(|i| (i % 251) as u8).collect();
        for direction in 0..2 {
            let (sender, receiver) = if direction == 0 { (&mut stream, &mut second) } else { (&mut second, &mut stream) };
            sender.send_raw(payload.clone()).await?;
            let mut received = Vec::new();
            while received.len() < payload.len() {
                received.extend_from_slice(&receiver.next_timeout(5000).await.context("relay payload missing")??);
            }
            anyhow::ensure!(received == payload, "relay payload mismatch");
        }
        println!("PASS authenticated {}{}: 131072 bidirectional bytes", endpoint.host, endpoint.path);
    } else {
        println!("PASS authenticated {}{}", endpoint.host, endpoint.path);
    }
    // Keep the type checked in this isolated probe too.
    let _: Stream = stream;
    Ok(())
}
use anyhow::Context;
