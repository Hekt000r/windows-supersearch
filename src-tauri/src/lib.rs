// src-tauri/src/lib.rs
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, ShortcutState};
use std::sync::{Arc, Mutex};
use rayon::prelude::*;

mod engine;

fn load_filenames_from_db() -> Vec<String> {
    let conn = match rusqlite::Connection::open("files.db") {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut stmt = match conn.prepare("SELECT name FROM files") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |row| row.get(0));
    match rows {
        Ok(r) => r.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

struct AppState {
    filenames: Arc<Mutex<Arc<Vec<String>>>>, // Inner Arc for cheap cloning on search
}

#[derive(Serialize)]
struct SearchResult {
    mft_entry: u64,
    name: String,
}

#[tauri::command]
fn search_files(query: &str, limit: usize, state: tauri::State<AppState>) -> Result<Vec<SearchResult>, String> {
    let query_lower = query.to_lowercase();
    let query_len = query_lower.len();

    // Lock and clone the Arc to release lock quickly
    let filenames_arc = state.filenames.lock().unwrap().clone();
    let filenames = &**filenames_arc; // &Vec<String>

    let mut results: Vec<_> = filenames
        .par_iter()
        .enumerate()
        .filter_map(|(idx, name)| {
            let name_lower = name.to_lowercase();

            // Find the first occurrence of the query in the filename
            if let Some(pos) = name_lower.find(&query_lower) {
                // Score: lower is better
                // 1. Exact match (pos 0, length equal) gets 0
                // 2. Prefix match (pos 0) gets 1
                // 3. Any other match gets pos + 1 (so "build.rs" at pos 0 scores 1, "ctest_build.rst" at pos 6 scores 7)
                let score = if pos == 0 && name_lower.len() == query_len {
                    0 // exact match
                } else if pos == 0 {
                    1 // prefix match
                } else {
                    pos + 1 // substring match, prefer earlier positions
                };

                // Store: (score, name, idx)
                Some((score, idx, name.clone()))
            } else {
                None
            }
        })
        .collect();

    // Sort by score (lower is better)
    results.sort_by_key(|(score, _, _)| *score);
    results.truncate(limit);

    let results: Vec<SearchResult> = results
        .into_iter()
        .map(|(_, idx, name)| SearchResult { mft_entry: idx as u64, name })
        .collect();

    Ok(results)
}

#[tauri::command]
fn open_file(mft_entry: u64) -> Result<(), String> {
    println!("Opening file with MFT entry: {}", mft_entry);
    Ok(())
}

#[tauri::command]
fn rescan_index(state: tauri::State<AppState>) -> Result<String, String> {
    // Run the scanner (populates DB)
    engine::advanced_scanner::open_volume_handle()
        .map_err(|e| e.to_string())?;

    // Reload filenames from DB into memory
    let new_filenames = load_filenames_from_db();
    let mut locked = state.filenames.lock().unwrap();
    *locked = Arc::new(new_filenames);

    Ok("Rescan completed".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load filenames from DB (if exists)
    let filenames = load_filenames_from_db();
    let state = AppState {
        filenames: Arc::new(Mutex::new(Arc::new(filenames))),
    };

    let shortcut_plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(
            |app: &tauri::AppHandle,
             shortcut: &tauri_plugin_global_shortcut::Shortcut,
             event: tauri_plugin_global_shortcut::ShortcutEvent| {
                if event.state == ShortcutState::Pressed
                    && shortcut.matches(Modifiers::CONTROL, Code::Space)
                {
                    let window = app.get_webview_window("main").unwrap();
                    if window.is_visible().unwrap() {
                        window.hide().unwrap();
                    } else {
                        window.show().unwrap();
                        window.set_focus().unwrap();
                    }
                }
            },
        )
        .build();

    tauri::Builder::default()
        .plugin(shortcut_plugin)
        .manage(state) // <-- ADD THIS
        .invoke_handler(tauri::generate_handler![
            search_files,
            open_file,
            rescan_index
        ])
        .setup(|app| {
            let app_handle = app.handle();
            app_handle
                .global_shortcut()
                .register("Ctrl+Space")
                .map_err(|e| e.to_string())?;
            let window = app_handle.get_webview_window("main").unwrap();
            window.set_always_on_top(true).unwrap();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}