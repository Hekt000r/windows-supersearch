// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
use crate::engine::scanner::open_volume_handle;

fn main() {
    // --- TEST BLOCK ---
    println!("--- Testing System Privileges ---");
    match open_volume_handle() {
        Ok(handle) => println!("SUCCESS: Got handle {:?}", handle),
        Err(e) => println!("FAILURE: {}", e),
    }
    println!("---------------------------------");
    
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
