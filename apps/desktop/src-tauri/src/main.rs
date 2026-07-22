#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(debug_assertions)]
    eprintln!(
        "[DeskAide] Debug console logging is enabled. Model prompts and responses may contain sensitive information.\n"
    );

    deskaide_desktop_lib::run();
}
