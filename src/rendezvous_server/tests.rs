use super::*;
use crate::database::test_support::TestDatabase;
use register_pk_response::Result::*;

fn server(db: crate::database::Database) -> RendezvousServer {
    let (tx, _) = mpsc::unbounded_channel();
    RendezvousServer {
        tcp_punch: Default::default(),
        pm: PeerMap::for_test(db),
        tx,
        relay_servers: Default::default(),
        relay_servers0: Default::default(),
        rendezvous_servers: Default::default(),
        inner: Arc::new(Inner {
            serial: 0,
            version: String::new(),
            software_url: String::new(),
            mask: None,
            local_ip: String::new(),
            sk: None,
        }),
    }
}

fn request(old_id: &str, id: &str, uuid: &'static [u8]) -> RendezvousMessage {
    let mut msg = RendezvousMessage::new();
    msg.set_register_pk(RegisterPk {
        old_id: old_id.into(),
        id: id.into(),
        uuid: Bytes::from_static(uuid),
        ..Default::default()
    });
    msg
}

fn result(bytes: &[u8]) -> register_pk_response::Result {
    match RendezvousMessage::parse_from_bytes(bytes)
        .unwrap()
        .union
        .unwrap()
    {
        rendezvous_message::Union::RegisterPkResponse(response) => {
            response.result.enum_value().unwrap()
        }
        other => panic!("Unexpected response: {other:?}"),
    }
}

async fn tcp_request(
    mut rs: RendezvousServer,
    msg: RendezvousMessage,
) -> register_pk_response::Result {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handler = tokio::spawn(async move {
        let (stream, remote) = listener.accept().await.unwrap();
        rs.handle_listener_inner(stream, remote, "", false)
            .await
            .unwrap();
    });
    let mut client = FramedStream::from(TcpStream::connect(addr).await.unwrap(), addr);
    client.send(&msg).await.unwrap();
    let bytes = client.next_timeout(2000).await.unwrap().unwrap();
    let response = result(&bytes);
    timeout(2000, handler).await.unwrap().unwrap();
    response
}

#[tokio::test]
async fn tcp_change_id_accepts_the_clients_request_without_a_public_key() {
    let fixture = TestDatabase::new().await;
    fixture
        .db
        .insert_peer("123456789", b"machine", b"key", "{}")
        .await
        .unwrap();
    fixture
        .db
        .insert_peer("Taken-ID", b"other", b"key2", "{}")
        .await
        .unwrap();
    let rs = server(fixture.db.clone());
    assert_eq!(
        tcp_request(rs.clone(), request("123456789", "123456", b"machine")).await,
        INVALID_ID_FORMAT
    );
    assert_eq!(
        tcp_request(rs.clone(), request("123456789", "Custom-ID", b"wrong")).await,
        UUID_MISMATCH
    );
    assert_eq!(
        tcp_request(rs.clone(), request("123456789", "Taken-ID", b"machine")).await,
        ID_EXISTS
    );
    assert_eq!(
        tcp_request(rs.clone(), request("123456789", "Custom-ID", b"machine")).await,
        OK
    );
    assert_eq!(
        tcp_request(rs.clone(), request("123456789", "Custom-ID", b"machine")).await,
        OK
    );
    assert_eq!(
        tcp_request(rs, request("Custom-ID", "Second-ID", b"machine")).await,
        TOO_FREQUENT
    );
    assert!(fixture.db.get_peer("123456789").await.unwrap().is_none());
    assert_eq!(
        fixture.db.get_peer("Custom-ID").await.unwrap().unwrap().pk,
        b"key"
    );
}

#[tokio::test]
async fn websocket_change_id_uses_the_same_protocol() {
    let fixture = TestDatabase::new().await;
    fixture
        .db
        .insert_peer("123456789", b"machine", b"key", "{}")
        .await
        .unwrap();
    let mut rs = server(fixture.db.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handler = tokio::spawn(async move {
        let (stream, remote) = listener.accept().await.unwrap();
        rs.handle_listener_inner(stream, remote, "", true)
            .await
            .unwrap();
    });
    let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{addr}"))
        .await
        .unwrap();
    client
        .send(tungstenite::Message::Binary(
            request("123456789", "Custom-ID", b"machine")
                .write_to_bytes()
                .unwrap(),
        ))
        .await
        .unwrap();
    let response = timeout(2000, client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result(&response.into_data()), OK);
    timeout(2000, handler).await.unwrap().unwrap();
    assert!(fixture.db.get_peer("Custom-ID").await.unwrap().is_some());
}

async fn udp_register(rs: &mut RendezvousServer, id: &str) -> register_pk_response::Result {
    let mut socket = FramedSocket::new("127.0.0.1:0").await.unwrap();
    let mut client = FramedSocket::new("127.0.0.1:0").await.unwrap();
    let mut msg = request("", id, b"machine");
    if let Some(rendezvous_message::Union::RegisterPk(rk)) = &mut msg.union {
        rk.pk = Bytes::from_static(b"key");
    }
    let bytes = BytesMut::from(msg.write_to_bytes().unwrap().as_slice());
    rs.handle_udp(&bytes, client.local_addr().unwrap(), &mut socket, "")
        .await
        .unwrap();
    let (response, _) = timeout(2000, client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    result(&response)
}

#[tokio::test]
async fn renamed_id_can_register_over_udp_and_reports_write_failures() {
    let fixture = TestDatabase::new().await;
    let mut rs = server(fixture.db.clone());
    assert_eq!(udp_register(&mut rs, "123456789").await, OK);
    assert_eq!(
        tcp_request(rs.clone(), request("123456789", "Custom-ID", b"machine")).await,
        OK
    );
    assert_eq!(udp_register(&mut rs, "Custom-ID").await, OK);
    let peer = rs.pm.get("Custom-ID").await.unwrap();
    let addr = peer.read().await.socket_addr;
    assert_eq!(peer.read().await.pk.as_ref(), b"key");
    assert!(rs.pm.get("123456789").await.is_none());
    sqlx::query("create trigger fail_register before update on peer begin select raise(abort, 'write failed'); end")
        .execute(&mut *fixture.db.test_connection().await).await.unwrap();
    // Force a regular registration to update the stored IP.
    peer.write().await.info.ip = "192.0.2.1".into();
    assert_eq!(udp_register(&mut rs, "Custom-ID").await, SERVER_ERROR);
    assert_eq!(peer.read().await.socket_addr, addr);
    assert_eq!(peer.read().await.info.ip, "192.0.2.1");
}
