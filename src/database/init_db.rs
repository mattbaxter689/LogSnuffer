use async_rusqlite::Connection;

use crate::log_generator::log_methods::LogEntry;

pub async fn init_db() -> Connection {
    let conn = Connection::open("db/snuff.db")
        .await
        .expect("Error connecting to database");

    conn.call(|c| {
        c.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER,
                level TEXT,
                message TEXT,
                instance TEXT,
                service TEXT
            );
            CREATE TABLE IF NOT EXISTS tickets (
                id INTEGER PRIMARY KEY,
                title TEXT,
                description TEXT,
                portal TEXT
            );
        ",
        )
    })
    .await
    .expect("Error creating tables");

    conn
}

pub async fn store_log(
    conn: Connection,
    log: LogEntry,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    conn.call(move |c| {
        c.execute(
            "INSERT INTO logs (timestamp, level, message, instance, service)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                log.timestamp,
                format!("{:?}", log.level),
                log.message,
                log.instance,
                log.service,
            ),
        )
    })
    .await?;

    Ok(())
}
