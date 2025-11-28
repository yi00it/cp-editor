//! CP Editor - GPU-accelerated text editor.
//!
//! Usage: cp-editor [FILE]

use cp_editor_ui::{run, EditorApp};
use std::env;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    // Start tracking startup time
    let startup_start = Instant::now();

    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting CP Editor");

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let file_path = args.get(1).map(PathBuf::from);

    // Create the application
    let mut app = EditorApp::new(16.0);

    // Open file if provided (replaces the default empty buffer)
    if let Some(ref path) = file_path {
        log::info!("Opening file: {:?}", path);
        if let Err(e) = app.workspace.open_file_in_current(path) {
            log::error!("Failed to open file '{:?}': {}", path, e);
        }
        app.perf_metrics.startup.record_file_open();
    }

    // Log startup time
    let startup_time = startup_start.elapsed();
    log::info!("Startup complete in {:.1}ms", startup_time.as_secs_f64() * 1000.0);

    // Run the application
    run(app);

    log::info!("CP Editor exited");
}
