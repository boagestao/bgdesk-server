use async_trait::async_trait;
use hbb_common::{log, ResultType};
use sqlx::{
    sqlite::SqliteConnectOptions, ConnectOptions, Connection, Error as SqlxError, SqliteConnection,
};
use std::{ops::DerefMut, str::FromStr};
//use sqlx::postgres::PgPoolOptions;
//use sqlx::mysql::MySqlPoolOptions;

type Pool = deadpool::managed::Pool<DbPool>;

pub struct DbPool {
    url: String,
}

#[async_trait]
impl deadpool::managed::Manager for DbPool {
    type Type = SqliteConnection;
    type Error = SqlxError;
    async fn create(&self) -> Result<SqliteConnection, SqlxError> {
        let mut opt = SqliteConnectOptions::from_str(&self.url).unwrap();
        opt.log_statements(log::LevelFilter::Debug);
        SqliteConnection::connect_with(&opt).await
    }
    async fn recycle(
        &self,
        obj: &mut SqliteConnection,
    ) -> deadpool::managed::RecycleResult<SqlxError> {
        Ok(obj.ping().await?)
    }
}

#[derive(Clone)]
pub struct Database {
    pool: Pool,
}

#[derive(Default)]
pub struct Peer {
    pub guid: Vec<u8>,
    pub id: String,
    pub uuid: Vec<u8>,
    pub pk: Vec<u8>,
    pub user: Option<Vec<u8>>,
    pub info: String,
    pub status: Option<i64>,
}

impl Database {
    #[cfg(test)]
    pub(crate) async fn test_connection(&self) -> deadpool::managed::Object<DbPool> {
        self.pool.get().await.unwrap()
    }

    pub async fn new(url: &str) -> ResultType<Database> {
        if !std::path::Path::new(url).exists() {
            std::fs::File::create(url).ok();
        }
        let n: usize = std::env::var("MAX_DATABASE_CONNECTIONS")
            .unwrap_or_else(|_| "1".to_owned())
            .parse()
            .unwrap_or(1);
        log::debug!("MAX_DATABASE_CONNECTIONS={}", n);
        let pool = Pool::new(
            DbPool {
                url: url.to_owned(),
            },
            n,
        );
        let _ = pool.get().await?; // test
        let db = Database { pool };
        db.create_tables().await?;
        Ok(db)
    }

    async fn create_tables(&self) -> ResultType<()> {
        sqlx::query!(
            "
            create table if not exists peer (
                guid blob primary key not null,
                id varchar(100) not null,
                uuid blob not null,
                pk blob not null,
                created_at datetime not null default(current_timestamp),
                user blob,
                status tinyint,
                note varchar(300),
                info text not null
            ) without rowid;
            create unique index if not exists index_peer_id on peer (id);
            create index if not exists index_peer_user on peer (user);
            create index if not exists index_peer_created_at on peer (created_at);
            create index if not exists index_peer_status on peer (status);
        "
        )
        .execute(self.pool.get().await?.deref_mut())
        .await?;
        Ok(())
    }

    pub async fn get_peer(&self, id: &str) -> ResultType<Option<Peer>> {
        Ok(sqlx::query_as!(
            Peer,
            "select guid, id, uuid, pk, user, status, info from peer where id = ?",
            id
        )
        .fetch_optional(self.pool.get().await?.deref_mut())
        .await?)
    }

    pub async fn insert_peer(
        &self,
        id: &str,
        uuid: &[u8],
        pk: &[u8],
        info: &str,
    ) -> ResultType<Vec<u8>> {
        let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
        sqlx::query!(
            "insert into peer(guid, id, uuid, pk, info) values(?, ?, ?, ?, ?)",
            guid,
            id,
            uuid,
            pk,
            info
        )
        .execute(self.pool.get().await?.deref_mut())
        .await?;
        Ok(guid)
    }

    pub async fn update_pk(
        &self,
        guid: &Vec<u8>,
        id: &str,
        pk: &[u8],
        info: &str,
    ) -> ResultType<()> {
        sqlx::query!(
            "update peer set id=?, pk=?, info=? where guid=?",
            id,
            pk,
            info,
            guid
        )
        .execute(self.pool.get().await?.deref_mut())
        .await?;
        Ok(())
    }

    // A single conditional UPDATE preserves the device's GUID and metadata and
    // lets the unique index arbitrate concurrent claims, including other processes.
    pub async fn change_id(
        &self,
        guid: &[u8],
        old_id: &str,
        id: &str,
        uuid: &[u8],
    ) -> ResultType<hbb_common::rendezvous_proto::register_pk_response::Result> {
        use hbb_common::rendezvous_proto::register_pk_response::Result::*;
        let result = sqlx::query("update peer set id=? where guid=? and id=? and uuid=?")
            .bind(id)
            .bind(guid)
            .bind(old_id)
            .bind(uuid)
            .execute(self.pool.get().await?.deref_mut())
            .await;
        match result {
            Ok(result) if result.rows_affected() == 1 => Ok(OK),
            Ok(_) => Ok(UUID_MISMATCH),
            // SQLx 0.6 exposes SQLite's extended SQLITE_CONSTRAINT_UNIQUE code.
            Err(SqlxError::Database(err)) if err.code().as_deref() == Some("2067") => Ok(ID_EXISTS),
            Err(err) => Err(err.into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) struct TestDatabase {
        pub(crate) db: super::Database,
        path: std::path::PathBuf,
    }

    impl TestDatabase {
        pub(crate) async fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("bgdesk-{}.sqlite3", uuid::Uuid::new_v4()));
            let db = super::Database::new(path.to_str().unwrap()).await.unwrap();
            Self { db, path }
        }
    }

    impl Drop for TestDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use hbb_common::tokio;
    #[test]
    fn test_insert() {
        insert();
    }

    #[tokio::main(flavor = "multi_thread")]
    async fn insert() {
        let fixture = super::test_support::TestDatabase::new().await;
        let db = &fixture.db;
        let mut jobs = vec![];
        for i in 0..10000 {
            let cloned = db.clone();
            let id = i.to_string();
            let a = tokio::spawn(async move {
                let empty_vec = Vec::new();
                cloned
                    .insert_peer(&id, &empty_vec, &empty_vec, "")
                    .await
                    .unwrap();
            });
            jobs.push(a);
        }
        for i in 0..10000 {
            let cloned = db.clone();
            let id = i.to_string();
            let a = tokio::spawn(async move {
                cloned.get_peer(&id).await.unwrap();
            });
            jobs.push(a);
        }
        for result in hbb_common::futures::future::join_all(jobs).await {
            result.unwrap();
        }
    }

    #[tokio::test]
    async fn change_id_is_conditional_and_preserves_metadata() {
        use hbb_common::rendezvous_proto::register_pk_response::Result::*;
        let fixture = super::test_support::TestDatabase::new().await;
        let db = &fixture.db;
        let guid = db
            .insert_peer("123456789", b"machine", b"key", "{}")
            .await
            .unwrap();
        let mut connection = db.pool.get().await.unwrap();
        sqlx::query("update peer set user=?, status=0, note='device note' where guid=?")
            .bind(b"user".as_slice())
            .bind(&guid)
            .execute(&mut *connection)
            .await
            .unwrap();
        drop(connection);
        assert_eq!(
            db.change_id(&guid, "123456789", "Custom-ID", b"wrong")
                .await
                .unwrap(),
            UUID_MISMATCH
        );
        assert_eq!(
            db.change_id(&guid, "123456789", "Custom-ID", b"machine")
                .await
                .unwrap(),
            OK
        );
        assert!(db.get_peer("123456789").await.unwrap().is_none());
        let peer = db.get_peer("Custom-ID").await.unwrap().unwrap();
        assert_eq!(peer.guid, guid);
        assert_eq!(peer.uuid, b"machine");
        assert_eq!(peer.pk, b"key");
        assert_eq!(peer.info, "{}");
        assert_eq!(peer.user, Some(b"user".to_vec()));
        assert_eq!(peer.status, Some(0));
        let note: String = sqlx::query_scalar("select note from peer where guid=?")
            .bind(&guid)
            .fetch_one(&mut *db.pool.get().await.unwrap())
            .await
            .unwrap();
        assert_eq!(note, "device note");
    }

    #[tokio::test]
    async fn change_id_handles_collisions_and_database_errors() {
        use hbb_common::rendezvous_proto::register_pk_response::Result::*;
        let fixture = super::test_support::TestDatabase::new().await;
        let db = &fixture.db;
        let guid = db
            .insert_peer("123456789", b"machine", b"key", "{}")
            .await
            .unwrap();
        db.insert_peer("Taken-ID", b"other", b"other key", "{}")
            .await
            .unwrap();
        assert_eq!(
            db.change_id(&guid, "123456789", "Taken-ID", b"machine")
                .await
                .unwrap(),
            ID_EXISTS
        );
        sqlx::query("create trigger fail_rename before update of id on peer begin select raise(abort, 'write failed'); end")
            .execute(&mut *db.pool.get().await.unwrap()).await.unwrap();
        assert!(db
            .change_id(&guid, "123456789", "Custom-ID", b"machine")
            .await
            .is_err());
        assert_eq!(db.get_peer("123456789").await.unwrap().unwrap().guid, guid);
        assert!(db.get_peer("Custom-ID").await.unwrap().is_none());
    }
}
