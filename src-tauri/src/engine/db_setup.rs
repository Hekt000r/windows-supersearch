use duckdb::{params, Connection};

pub fn setup_db(conn: &Connection) -> duckdb::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS files (
            file_id BIGINT PRIMARY KEY,
            parent_id BIGINT,
            name VARCHAR
        )",
        [],
    )?;
    Ok(())
}
