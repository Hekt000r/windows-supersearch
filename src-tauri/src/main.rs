// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
use crate::engine::scanner::scan_volume;
use crate::engine::db_setup::setup_db;

fn main() {
    println!("--- Running inital scan ---");
    match scan_volume() {
        Ok(handle) => println!("SUCCESS: Got handle {:?}", handle),
        Err(e) => println!("FAILURE: {}", e),
    }
    println!("---------------------------------");
    
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
