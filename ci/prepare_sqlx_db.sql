-- Schema used by sqlx compile-time query checking.
-- The resulting database is committed as sqlx-compile.sqlite3 for local `cargo sqlx prepare`.
-- After changing queries in src/database.rs, regenerate offline metadata with:
--   DATABASE_URL="sqlite://$(pwd)/ci/sqlx-compile.sqlite3" cargo sqlx prepare --merged -- --lib
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
