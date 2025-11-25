// ‼️ Refactored: This acts as the facade. exposing submodules.
pub mod audio_capture;
pub mod audio_player;
pub mod events;
pub mod state;
pub mod ui;

use crate::app::audio_capture::run_capture_loop;
use crate::app::audio_player::run_kira_loop;
use crate::app::state::{AppCommand, AppState, AudioCommand};
use log::{error, info};
use push2::Push2;
use std::error::Error;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

// ‼️ Refactored: Main application logic extracted here
pub async fn run() -> Result<(), Box<dyn Error>> {
    // 1. Setup Channels
    let (audio_tx, audio_rx) = mpsc::channel::<AudioCommand>();
    let (app_tx, app_rx) = mpsc::channel::<AppCommand>();
    let (kira_tx, kira_rx) = mpsc::channel::<audio_player::KiraCommand>();

    // 2. Spawn Audio Threads
    spawn_audio_threads(audio_rx, app_tx.clone(), kira_rx);

    // 3. Initialize Hardware & State
    let mut push2 = Push2::new()?;
    let mut app_state = AppState::new(audio_tx, kira_tx)?;

    // 4. Initial Hardware Setup
    initial_hardware_setup(&mut push2, &mut app_state)?;

    info!("System Ready. Starting Main Loop.");

    // 5. Main Loop
    loop {
        // ‼️ Refactored: Event handling extracted to events.rs
        events::handle_incoming_events(&mut push2, &mut app_state, &app_rx).await?;

        // ‼️ Refactored: UI Drawing extracted to ui.rs
        if let Err(e) = ui::draw_screen(&mut push2, &mut app_state) {
            error!("Display error: {}", e);
            break;
        }

        // Maintain frame rate
        tokio::time::sleep(Duration::from_millis(1000 / 60)).await;
    }

    Ok(())
}

// ‼️ Refactored: Extracted helper for spawning threads
fn spawn_audio_threads(
    audio_rx: mpsc::Receiver<AudioCommand>,
    app_tx: mpsc::Sender<AppCommand>,
    kira_rx: mpsc::Receiver<audio_player::KiraCommand>,
) {
    thread::spawn(move || {
        info!("Audio capture thread started...");
        if let Err(e) = run_capture_loop(audio_rx, app_tx) {
            error!("Audio capture thread failed: {}", e);
        }
    });

    thread::spawn(move || {
        info!("Kira audio thread started...");
        if let Err(e) = run_kira_loop(kira_rx) {
            error!("Kira audio thread failed: {}", e);
        }
    });
}

// ‼️ Refactored: Extracted helper for initial LED setup
fn initial_hardware_setup(push2: &mut Push2, state: &mut AppState) -> Result<(), Box<dyn Error>> {
    state.update_pad_lights(push2)?;
    // Set initial button states
    push2.set_button_light(push2::ControlName::Mute, push2::Push2Colors::GREEN_PALE)?;
    push2.set_button_light(push2::ControlName::Solo, push2::Push2Colors::GREEN_PALE)?;
    Ok(())
}
