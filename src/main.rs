// ‼️ Refactored: Minimal entry point. All logic moved to app module.
mod app;

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    // ‼️ Refactored: definition of main loop is now in app::run
    app::run().await
}
