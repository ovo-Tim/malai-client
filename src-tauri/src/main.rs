// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use tracing::Level;
use tracing_subscriber;

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    malai_client_lib::run()
}
