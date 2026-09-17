// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Elevated helper for Stability settings: apply the changes and exit without a window.
    if let Some(code) = task_manager_lib::settings::run_helper_from_args() {
        std::process::exit(code);
    }
    task_manager_lib::run()
}
