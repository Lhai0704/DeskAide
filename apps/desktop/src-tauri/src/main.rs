// Debug builds keep a console window for logs (`eprintln!`, panic messages).
// Release builds hide it so double-clicking the exe only shows the avatar.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    deskaide_desktop_lib::run();
}
