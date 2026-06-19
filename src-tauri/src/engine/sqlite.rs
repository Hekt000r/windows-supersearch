use rayon::prelude::*;
use rusqlite::{params, Connection, Result as SqliteResult};

pub fn init_database() -> SqliteResult<Connection> {
    let conn = Connection::open("files.db")?;

    // ---- BULK INSERT OPTIMIZATIONS ----
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = OFF;
         PRAGMA cache_size = -1000000;
         PRAGMA temp_store = MEMORY;",
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS files (
            mft_entry INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(name)",
        [],
    )?;

    Ok(conn)
}

// FIX 1: Change `&Connection` to `&mut Connection`
pub fn decode_and_insert(
    raw_buffer: Vec<(u64, Vec<u8>)>,
    conn: &mut Connection, // <-- Mutable reference
) -> SqliteResult<()> {
    println!("Decoding {} filenames in parallel...", raw_buffer.len());

    // Phase 2a: Parallel decode to Strings
    let decoded: Vec<(u64, String)> = raw_buffer
        .par_iter()
        .map(|(entry, raw_bytes)| {
            let utf16_units: Vec<u16> = raw_bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            let name = String::from_utf16_lossy(&utf16_units);
            (*entry, name)
        })
        .collect();

    println!(
        "Decoded {} filenames. Inserting into database...",
        decoded.len()
    );

    // Phase 2b: Batch insert into SQLite
    let tx = conn.transaction()?;

    // FIX 2: Drop `stmt` before committing `tx`
    {
        let mut stmt = tx.prepare_cached("INSERT INTO files (mft_entry, name) VALUES (?, ?)")?;

        let batch_size = 10000;
        for chunk in decoded.chunks(batch_size) {
            for (entry, name) in chunk {
                stmt.execute(params![entry, name])?;
            }
            // Optional progress print
            // println!("Inserted {} records...", chunk.len());
        }
        // `stmt` is dropped here when it goes out of scope
    }

    tx.commit()?;

    println!("Insert complete!");
    Ok(())
}

pub fn build_fts_index(conn: &mut Connection) -> SqliteResult<()> {
    println!("Building full‑text index...");
    let tx = conn.transaction()?;
    
    // Insert all names into the FTS table, linking by rowid (which matches mft_entry)
    // Since we used mft_entry as the primary key, and FTS5 uses rowid,
    // we can copy the data directly.
    tx.execute(
        "INSERT INTO files_fts (rowid, name) SELECT mft_entry, name FROM files",
        [],
    )?;
    
    tx.commit()?;
    println!("Full‑text index built.");
    Ok(())
}