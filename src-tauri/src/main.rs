// Keep the Windows release application from opening a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    handraw_style_codex_lib::run()
}
