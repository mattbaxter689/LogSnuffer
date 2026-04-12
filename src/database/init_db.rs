use async_rusqlite::{Connection, rusqlite};
use crate::log_generator::log_methods::LogEntry;

pub async fn init_db() -> Connection {
    let conn = Connection::open("db/logs.db")
        .await
        .expect("Failed to open database");

    conn.call(|conn| {
        // Logs table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                service TEXT NOT NULL,
                message TEXT NOT NULL,
                level TEXT NOT NULL,
                instance TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            )",
            [],
        )?;
        
        // GitHub issues tracking table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS github_issues (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                issue_number INTEGER NOT NULL UNIQUE,
                title TEXT NOT NULL,
                body TEXT,
                error_pattern TEXT NOT NULL,
                state TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                closed_at INTEGER,
                related_issues TEXT
            )",
            [],
        )?;
        
        // Warnings/monitoring table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS warnings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                error_pattern TEXT NOT NULL,
                severity TEXT NOT NULL,
                description TEXT NOT NULL,
                first_seen INTEGER NOT NULL,
                last_seen INTEGER NOT NULL,
                occurrence_count INTEGER NOT NULL,
                status TEXT NOT NULL
            )",
            [],
        )?;

        // Sessions auditing table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS session_audits (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                confidence_score INTEGER NOT NULL,
                ingestion_feedback TEXT,
                internal_monologue TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP)",
            [],
        )?;
        Ok::<(), rusqlite::Error>(())
    })
    .await
    .expect("Failed to create tables");

    conn
}

pub async fn store_log(conn: Connection, log: LogEntry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO logs (service, message, level, instance, timestamp) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                log.service,
                log.message,
                format!("{:?}", log.level),
                log.instance,
                log.timestamp as i64,
            ],
        )?;
        Ok::<(), rusqlite::Error>(())
    })
    .await?;

    Ok(())
}

pub async fn store_github_issue(
    conn: Connection,
    issue_number: u64,
    title: String,
    body: Option<String>,
    error_pattern: String,
    state: String,
    related_issues: Vec<u64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let related = serde_json::to_string(&related_issues)?;
    let now = chrono::Utc::now().timestamp();
    
    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO github_issues 
             (issue_number, title, body, error_pattern, state, created_at, related_issues)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                issue_number as i64,
                title,
                body,
                error_pattern,
                state,
                now,
                related,
            ],
        )?;
        Ok::<(), rusqlite::Error>(())
    })
    .await?;

    Ok(())
}

pub async fn update_issue_state(
    conn: Connection,
    issue_number: u64,
    state: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = chrono::Utc::now().timestamp();
    
    conn.call(move |conn| {
        conn.execute(
            "UPDATE github_issues 
             SET state = ?1, closed_at = ?2
             WHERE issue_number = ?3",
            rusqlite::params![state, now, issue_number as i64],
        )?;
        Ok::<(), rusqlite::Error>(())
    })
    .await?;

    Ok(())
}

pub async fn store_warning(
    conn: Connection,
    error_pattern: String,
    severity: String,
    description: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = chrono::Utc::now().timestamp();
    
    conn.call(move |conn| {
        // Check if warning exists
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM warnings WHERE error_pattern = ?1 AND status = 'active'",
                [&error_pattern],
                |row| {
                    let count: i64 = row.get(0)?;
                    Ok(count > 0)
                },
            )?;
        
        if exists {
            // Update existing warning
            conn.execute(
                "UPDATE warnings 
                 SET last_seen = ?1, occurrence_count = occurrence_count + 1
                 WHERE error_pattern = ?2 AND status = 'active'",
                rusqlite::params![now, &error_pattern],
            )?;
        } else {
            // Insert new warning
            conn.execute(
                "INSERT INTO warnings 
                 (error_pattern, severity, description, first_seen, last_seen, occurrence_count, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, 'active')",
                rusqlite::params![error_pattern, severity, description, now, now],
            )?;
        }
        
        Ok::<(), rusqlite::Error>(())
    })
    .await?;

    Ok(())
}

pub async fn get_recent_lessons(conn: Connection) -> String {
    match conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT ingestion_feedback, internal_monologue 
             FROM session_audits 
             WHERE confidence_score < 7 
             ORDER BY created_at DESC LIMIT 3"
        )?;

        let rows = stmt.query_map([], |row| {
            let feedback: Option<String> = row.get(0)?;
            let monologue: String = row.get(1)?;

            Ok(format!(
                "- Issue: {}\n  Context: {}", 
                feedback.unwrap_or_else(|| "N/A".to_string()), 
                monologue
            ))
        })?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;

        Ok::<String, rusqlite::Error>(rows.join("\n"))
    }).await {
        Ok(lessons) if !lessons.is_empty() => lessons,
        _ => "No major issues reported in recent sessions.".to_string(),
    }
}

pub async fn store_session_audit(
    conn: Connection,
    session_id: String,
    confidence_score: i32,
    ingestion_feedback: Option<String>,
    internal_monologue: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO session_audits (session_id, confidence_score, ingestion_feedback, internal_monologue) 
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, confidence_score, ingestion_feedback, internal_monologue],
        )?;
        Ok::<(), rusqlite::Error>(())
    })
    .await?;

    Ok(())
}
