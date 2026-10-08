//! The `freekerf-desktop` binary.

// no console window on Windows release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    freekerf_desktop::run();
}
