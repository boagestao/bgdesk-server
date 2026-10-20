use crate::common::*;
use crate::database;
use hbb_common::{
    bytes::Bytes,
    log,
    rendezvous_proto::*,
    tokio::sync::{Mutex, RwLock},
    ResultType,
};
use serde_derive::{Deserialize, Serialize};
use std::{collections::HashMap, collections::HashSet, net::SocketAddr, sync::Arc, time::Instant};

type IpBlockMap = HashMap<String, ((u32, Instant), (HashSet<String>, Instant))>;
type UserStatusMap = HashMap<Vec<u8>, Arc<(Option<Vec<u8>>, bool)>>;
type IpChangesMap = HashMap<String, (Instant, HashMap<String, i32>)>;
lazy_static::lazy_static! {
    pub(crate) static ref IP_BLOCKER: Mutex<IpBlockMap> = Default::default();
    pub(crate) static ref USER_STATUS: RwLock<UserStatusMap> = Default::default();
    pub(crate) static ref IP_CHANGES: Mutex<IpChangesMap> = Default::default();
}
pub const IP_CHANGE_DUR: u64 = 180;
pub const IP_CHANGE_DUR_X2: u64 = IP_CHANGE_DUR * 2;
pub const DAY_SECONDS: u64 = 3600 * 24;
pub const IP_BLOCK_DUR: u64 = 60;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub(crate) struct PeerInfo {
    #[serde(default)]
    pub(crate) ip: String,
}

pub(crate) struct Peer {
    pub(crate) socket_addr: SocketAddr,
    pub(crate) last_reg_time: Instant,
    pub(crate) guid: Vec<u8>,
    pub(crate) uuid: Bytes,
    pub(crate) pk: Bytes,
    // pub(crate) user: Option<Vec<u8>>,
    pub(crate) info: PeerInfo,
    // pub(crate) disabled: bool,
    pub(crate) reg_pk: (u32, Instant), // how often register_pk
    last_id_change: Option<Instant>,
}

impl Default for Peer {
    fn default() -> Self {
        Self {
            socket_addr: "0.0.0.0:0".parse().unwrap(),
            last_reg_time: get_expired_time(),
            guid: Vec::new(),
            uuid: Bytes::new(),
            pk: Bytes::new(),
            info: Default::default(),
            // user: None,
            // disabled: false,
            reg_pk: (0, get_expired_time()),
            last_id_change: None,
        }
    }
}

pub(crate) type LockPeer = Arc<RwLock<Peer>>;

#[derive(Clone)]
pub(crate) struct PeerMap {
    map: Arc<RwLock<HashMap<String, LockPeer>>>,
    // Shared by UDP registration and TCP/WS renaming, including server clones.
    pub(crate) registration_lock: Arc<Mutex<()>>,
    pub(crate) db: database::Database,
}

impl PeerMap {
    #[cfg(test)]
    pub(crate) fn for_test(db: database::Database) -> Self {
        Self {
            map: Default::default(),
            registration_lock: Default::default(),
            db,
        }
    }

    pub(crate) async fn new() -> ResultType<Self> {
        let db = std::env::var("DB_URL").unwrap_or({
            let mut db = "db_v2.sqlite3".to_owned();
            #[cfg(all(windows, not(debug_assertions)))]
            {
                if let Some(path) = hbb_common::config::Config::icon_path().parent() {
                    db = format!("{}\\{}", path.to_str().unwrap_or("."), db);
                }
            }
            #[cfg(not(windows))]
            {
                db = format!("./{db}");
            }
            db
        });
        log::info!("DB_URL={}", db);
        let pm = Self {
            map: Default::default(),
            registration_lock: Default::default(),
            db: database::Database::new(&db).await?,
        };
        Ok(pm)
    }

    #[inline]
    pub(crate) async fn update_pk(
        &mut self,
        id: String,
        peer: LockPeer,
        addr: SocketAddr,
        uuid: Bytes,
        pk: Bytes,
        ip: String,
    ) -> register_pk_response::Result {
        log::info!("update_pk {} {:?} {:?} {:?}", id, addr, uuid, pk);
        let mut w = peer.write().await;
        let mut info = w.info.clone();
        info.ip = ip;
        let info_str = serde_json::to_string(&info).unwrap_or_default();
        if w.guid.is_empty() {
            match self.db.insert_peer(&id, &uuid, &pk, &info_str).await {
                Err(err) => {
                    log::error!("db.insert_peer failed: {}", err);
                    return register_pk_response::Result::SERVER_ERROR;
                }
                Ok(guid) => {
                    w.guid = guid;
                }
            }
        } else {
            if let Err(err) = self.db.update_pk(&w.guid, &id, &pk, &info_str).await {
                log::error!("db.update_pk failed: {}", err);
                return register_pk_response::Result::SERVER_ERROR;
            }
            log::info!("pk updated instead of insert");
        }
        w.socket_addr = addr;
        w.uuid = uuid;
        w.pk = pk;
        w.last_reg_time = Instant::now();
        w.info = info;
        register_pk_response::Result::OK
    }

    pub(crate) async fn change_id(
        &self,
        old_id: &str,
        id: &str,
        uuid: &[u8],
    ) -> register_pk_response::Result {
        use register_pk_response::Result::*;
        // Match the client's documented ASCII rules (Rust regex \w also accepts Unicode).
        if !(6..=16).contains(&id.len())
            || !id.as_bytes()[0].is_ascii_alphabetic()
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            || old_id.is_empty()
        {
            return INVALID_ID_FORMAT;
        }
        if uuid.is_empty() {
            return UUID_MISMATCH;
        }

        let _registration = self.registration_lock.lock().await;
        // Keep the cache locked through the database change so a concurrent get()
        // cannot repopulate the old ID from a stale database read.
        let mut map = self.map.write().await;
        let peer = if let Some(peer) = map.get(old_id) {
            peer.clone()
        } else {
            match self.db.get_peer(old_id).await {
                Ok(Some(peer)) => Self::from_database(peer),
                Ok(None) => {
                    // A retry after a lost response is successful only for the owner
                    // of the destination. No state is changed in this case.
                    return match self.db.get_peer(id).await {
                        Ok(Some(peer)) if peer.uuid == uuid => OK,
                        Ok(_) => UUID_MISMATCH,
                        Err(err) => {
                            log::error!("db.get_peer failed during ID change: {}", err);
                            SERVER_ERROR
                        }
                    };
                }
                Err(err) => {
                    log::error!("db.get_peer failed during ID change: {}", err);
                    return SERVER_ERROR;
                }
            }
        };
        let mut device = peer.write().await;
        if device.uuid.as_ref() != uuid || device.guid.is_empty() {
            return UUID_MISMATCH;
        }
        if old_id == id {
            return OK;
        }
        if let Some(target) = map.get(id) {
            if !target.read().await.uuid.is_empty() {
                return ID_EXISTS;
            }
        }
        if device
            .last_id_change
            .map_or(false, |tm| tm.elapsed().as_secs() < 60)
        {
            return TOO_FREQUENT;
        }
        match self.db.change_id(&device.guid, old_id, id, uuid).await {
            Ok(OK) => {
                device.last_id_change = Some(Instant::now());
                map.remove(old_id);
                map.insert(id.to_owned(), peer.clone());
                log::info!("Peer ID changed: {} -> {}", old_id, id);
                OK
            }
            Ok(result) => result,
            Err(err) => {
                log::error!("db.change_id failed: {}", err);
                SERVER_ERROR
            }
        }
    }

    fn from_database(v: database::Peer) -> LockPeer {
        Arc::new(RwLock::new(Peer {
            guid: v.guid,
            uuid: v.uuid.into(),
            pk: v.pk.into(),
            info: serde_json::from_str::<PeerInfo>(&v.info).unwrap_or_default(),
            ..Default::default()
        }))
    }

    #[inline]
    pub(crate) async fn get(&self, id: &str) -> Option<LockPeer> {
        if let Some(peer) = self.get_in_memory(id).await {
            return Some(peer);
        }
        let mut map = self.map.write().await;
        if let Some(peer) = map.get(id) {
            return Some(peer.clone());
        }
        if let Ok(Some(v)) = self.db.get_peer(id).await {
            let peer = Self::from_database(v);
            map.insert(id.to_owned(), peer.clone());
            return Some(peer);
        }
        None
    }

    #[inline]
    pub(crate) async fn get_or(&self, id: &str) -> LockPeer {
        if let Some(p) = self.get(id).await {
            return p;
        }
        let mut w = self.map.write().await;
        if let Some(p) = w.get(id) {
            return p.clone();
        }
        let tmp = LockPeer::default();
        w.insert(id.to_owned(), tmp.clone());
        tmp
    }

    #[inline]
    pub(crate) async fn get_in_memory(&self, id: &str) -> Option<LockPeer> {
        self.map.read().await.get(id).cloned()
    }

    #[inline]
    pub(crate) async fn is_in_memory(&self, id: &str) -> bool {
        self.map.read().await.contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::test_support::TestDatabase;
    use hbb_common::tokio;
    use register_pk_response::Result::*;

    #[tokio::test]
    async fn change_id_moves_live_peer_and_survives_cache_reload() {
        let fixture = TestDatabase::new().await;
        let mut pm = PeerMap::for_test(fixture.db.clone());
        let peer = pm.get_or("123456789").await;
        let addr = "192.0.2.1:1234".parse().unwrap();
        assert_eq!(
            pm.update_pk(
                "123456789".into(),
                peer.clone(),
                addr,
                Bytes::from_static(b"machine"),
                Bytes::from_static(b"public key"),
                "192.0.2.1".into()
            )
            .await,
            OK
        );
        let guid = peer.read().await.guid.clone();
        let registered = peer.read().await.last_reg_time;

        assert_eq!(
            pm.change_id("123456789", "Custom-ID_123456", b"machine")
                .await,
            OK
        );
        assert!(pm.get("123456789").await.is_none());
        let renamed = pm.get("Custom-ID_123456").await.unwrap();
        assert!(Arc::ptr_eq(&peer, &renamed));
        assert_eq!(renamed.read().await.socket_addr, addr);
        assert_eq!(renamed.read().await.last_reg_time, registered);

        let reloaded = PeerMap::for_test(fixture.db.clone());
        assert!(reloaded.get("123456789").await.is_none());
        let persisted = reloaded.get("Custom-ID_123456").await.unwrap();
        let persisted = persisted.read().await;
        assert_eq!(persisted.guid, guid);
        assert_eq!(persisted.uuid.as_ref(), b"machine");
        assert_eq!(persisted.pk.as_ref(), b"public key");
        assert_eq!(persisted.info.ip, "192.0.2.1");
    }

    #[tokio::test]
    async fn change_id_validates_format_and_ownership() {
        let fixture = TestDatabase::new().await;
        fixture
            .db
            .insert_peer("123456789", b"machine", b"key", "{}")
            .await
            .unwrap();
        let pm = PeerMap::for_test(fixture.db.clone());
        for id in [
            "",
            "Short",
            "Abcdefghijklmnopq",
            "123456",
            "_abcde",
            "-abcde",
            "Ab cd1",
            "Ab.cd1",
            "Aébcde",
            "Abcde\n",
        ] {
            assert_eq!(
                pm.change_id("123456789", id, b"machine").await,
                INVALID_ID_FORMAT,
                "{id:?}"
            );
        }
        assert_eq!(
            pm.change_id("", "Custom-ID", b"machine").await,
            INVALID_ID_FORMAT
        );
        for uuid in [b"".as_slice(), b"wrong"] {
            assert_eq!(
                pm.change_id("123456789", "Custom-ID", uuid).await,
                UUID_MISMATCH
            );
            assert_eq!(
                pm.change_id("123456789", "123456789", uuid).await,
                INVALID_ID_FORMAT
            );
        }
        assert_eq!(
            pm.change_id("missing", "Custom-ID", b"machine").await,
            UUID_MISMATCH
        );
        assert!(fixture.db.get_peer("123456789").await.unwrap().is_some());
        assert!(fixture.db.get_peer("Custom-ID").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn change_id_rejects_occupied_ids_even_for_the_same_machine() {
        let fixture = TestDatabase::new().await;
        fixture
            .db
            .insert_peer("123456789", b"machine", b"key", "{}")
            .await
            .unwrap();
        fixture
            .db
            .insert_peer("Taken-ID", b"machine", b"other key", "{}")
            .await
            .unwrap();
        let pm = PeerMap::for_test(fixture.db.clone());
        assert_eq!(
            pm.change_id("123456789", "Taken-ID", b"machine").await,
            ID_EXISTS
        );
        assert_eq!(
            pm.get("123456789").await.unwrap().read().await.pk.as_ref(),
            b"key"
        );
        assert_eq!(
            pm.get("Taken-ID").await.unwrap().read().await.pk.as_ref(),
            b"other key"
        );
    }

    #[tokio::test]
    async fn change_id_limits_repeated_changes_but_allows_retries() {
        let fixture = TestDatabase::new().await;
        fixture
            .db
            .insert_peer("123456789", b"machine", b"key", "{}")
            .await
            .unwrap();
        let pm = PeerMap::for_test(fixture.db.clone());
        assert_eq!(pm.change_id("123456789", "Custom-ID", b"machine").await, OK);
        assert_eq!(pm.change_id("123456789", "Custom-ID", b"machine").await, OK);
        assert_eq!(
            pm.change_id("123456789", "Custom-ID", b"wrong").await,
            UUID_MISMATCH
        );
        assert_eq!(pm.change_id("Custom-ID", "Custom-ID", b"machine").await, OK);
        assert_eq!(
            pm.change_id("Custom-ID", "Custom-ID", b"wrong").await,
            UUID_MISMATCH
        );
        assert_eq!(
            pm.change_id("Custom-ID", "Second-ID", b"machine").await,
            TOO_FREQUENT
        );
        let peer = pm.get("Custom-ID").await.unwrap();
        peer.write().await.last_id_change =
            Some(Instant::now() - std::time::Duration::from_secs(61));
        assert_eq!(pm.change_id("Custom-ID", "Abc_12", b"machine").await, OK);
    }

    #[tokio::test]
    async fn change_id_serializes_competing_claims_and_cache_loads() {
        let fixture = TestDatabase::new().await;
        fixture
            .db
            .insert_peer("123456789", b"machine1", b"key1", "{}")
            .await
            .unwrap();
        fixture
            .db
            .insert_peer("987654321", b"machine2", b"key2", "{}")
            .await
            .unwrap();
        let pm = PeerMap::for_test(fixture.db.clone());
        let (a, b, _) = tokio::join!(
            pm.change_id("123456789", "Custom-ID", b"machine1"),
            pm.change_id("987654321", "Custom-ID", b"machine2"),
            pm.get("123456789"),
        );
        assert!((a == OK && b == ID_EXISTS) || (a == ID_EXISTS && b == OK));
        let (old_id, loser_id, winner_uuid) = if a == OK {
            ("123456789", "987654321", b"machine1")
        } else {
            ("987654321", "123456789", b"machine2")
        };
        assert!(pm.get(old_id).await.is_none());
        assert!(fixture.db.get_peer(old_id).await.unwrap().is_none());
        assert!(pm.get(loser_id).await.is_some());
        assert_eq!(
            pm.get("Custom-ID")
                .await
                .unwrap()
                .read()
                .await
                .uuid
                .as_ref(),
            winner_uuid
        );
    }

    #[tokio::test]
    async fn failed_change_keeps_cache_and_database_intact() {
        let fixture = TestDatabase::new().await;
        fixture
            .db
            .insert_peer("123456789", b"machine", b"key", "{}")
            .await
            .unwrap();
        let pm = PeerMap::for_test(fixture.db.clone());
        let original = pm.get("123456789").await.unwrap();
        sqlx::query("create trigger fail_rename before update of id on peer begin select raise(abort, 'write failed'); end")
            .execute(&mut *fixture.db.test_connection().await).await.unwrap();
        assert_eq!(
            pm.change_id("123456789", "Custom-ID", b"machine").await,
            SERVER_ERROR
        );
        assert!(Arc::ptr_eq(&original, &pm.get("123456789").await.unwrap()));
        assert!(pm.get_in_memory("Custom-ID").await.is_none());
        assert!(fixture.db.get_peer("123456789").await.unwrap().is_some());
        assert!(fixture.db.get_peer("Custom-ID").await.unwrap().is_none());
        // A failed database lookup must not turn into an unowned ID or success.
        sqlx::query("drop table peer")
            .execute(&mut *fixture.db.test_connection().await)
            .await
            .unwrap();
        let fresh = PeerMap::for_test(fixture.db.clone());
        assert_eq!(
            fresh.change_id("123456789", "Custom-ID", b"machine").await,
            SERVER_ERROR
        );
    }
}
