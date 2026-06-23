use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use rayon::prelude::*;
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, ShortcutState};

mod engine;

// ===== State =====
struct AppState {
    // MFT entry → filename
    filenames: Arc<Mutex<Arc<HashMap<u64, String>>>>,
}

// ===== Search Result =====
#[derive(Serialize)]
struct SearchResult {
    mft_entry: u64,
    name: String,
}

// ===== Helper: Load from DB =====
fn load_filenames_from_db() -> HashMap<u64, String> {
    let conn = match rusqlite::Connection::open("files.db") {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let mut stmt = match conn.prepare("SELECT mft_entry, name FROM files") {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)));
    match rows {
        Ok(r) => r.filter_map(|r| r.ok()).collect(),
        Err(_) => HashMap::new(),
    }
}

// ===== Commands =====
#[tauri::command]
fn search_files(query: &str, limit: usize, state: tauri::State<AppState>) -> Result<Vec<SearchResult>, String> {
    let query_lower = query.to_lowercase();
    let query_len = query_lower.len();

    // Lock and clone to release quickly
    let map_arc = state.filenames.lock().unwrap().clone();
    let map: &HashMap<u64, String> = &*map_arc; // ✅ Deref once

    let mut results: Vec<_> = map
        .par_iter()
        .filter_map(|(id, name)| {
            let name_lower = name.to_lowercase();
            if let Some(pos) = name_lower.find(&query_lower) {
                let score = if pos == 0 && name_lower.len() == query_len {
                    0
                } else if pos == 0 {
                    1
                } else {
                    pos + 1
                };
                Some((score, *id, name.clone()))
            } else {
                None
            }
        })
        .collect();

    results.sort_by_key(|(score, id, _)| (*score, *id));
    results.truncate(limit);

    Ok(results
        .into_iter()
        .map(|(_, id, name)| SearchResult { mft_entry: id, name })
        .collect())
}

#[tauri::command]
fn open_file(mft_entry: u64) -> Result<(), String> {
    // TODO: implement file opening
    println!("Opening file with MFT entry: {}", mft_entry);
    Ok(())
}

#[tauri::command]
fn rescan_index(state: tauri::State<AppState>) -> Result<String, String> {
    engine::advanced_scanner::open_volume_handle()
        .map_err(|e| e.to_string())?;

    let new_map = load_filenames_from_db();
    let mut locked = state.filenames.lock().unwrap();
    *locked = Arc::new(new_map);

    Ok("Rescan completed".to_string())
}

// ===== App entry =====
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
        .manage(state)
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