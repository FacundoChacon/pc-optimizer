#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = pc_optimizer_lib::run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
