// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
use engine::usn_journal;

fn main() {
    match usn_journal::realtime_usn() {
        Ok(_) => println!("realtime_usn exited"),
        Err(e) => eprintln!("USN error: {}", e),
    }

    windows_supersearch_lib::run();
}
