use async_rusqlite::Connection;

pub async fn init_db() -> Connection {
    let conn = Connection::open("db/snuff.db")
        .await
        .expect("Error connecting to database");

    conn.call(|c| c.execute)
}
