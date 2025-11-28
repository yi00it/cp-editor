//! CP Editor - GPU-accelerated text editor.
//!
//! Usage: cp-editor [FILE]

use cp_editor_ui::{run, EditorApp};
use std::env;
use std::path::PathBuf;

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting CP Editor");

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let file_path = args.get(1).map(PathBuf::from);

    // Create the application
    let mut app = EditorApp::new(16.0);

    // Open file if provided (replaces the default empty buffer)
    if let Some(path) = file_path {
        log::info!("Opening file: {:?}", path);
        if let Err(e) = app.workspace.open_file_in_current(&path) {
            log::error!("Failed to open file '{:?}': {}", path, e);
        }
    }

    // Run the application
    run(app);

    log::info!("CP Editor exited");
}
