// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `elevant-mail mcp`: the MCP server for AI agents on stdin and stdout, without a window.
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        std::process::exit(elevant_mail_lib::run_mcp());
    }
    elevant_mail_lib::run();
}
