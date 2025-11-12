pub mod audio_capture;
pub mod audio_player;
pub mod audio_processor;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Serialize, Deserialize, Debug)]
pub enum AudioCommand {
    Start(PathBuf),
    Stop,
}
pub fn get_audio_storage_path() -> std::io::Result<PathBuf> {
    match dirs::audio_dir() {
        Some(mut path) => {
            path.push("soundboard-recordings");
            std::fs::create_dir_all(&path)?;
            Ok(path)
        }
        None => Err(std::io::Error::other("Could not find audio directory")),
    }
}
// --- Original main.rs content below ---
use crate::audio_player::PlaybackSink;
use embedded_graphics::{pixelcolor::Bgr565, prelude::*};
use log::{debug, info};
use push2::{ControlName, EncoderName, GuiApi, PadCoord, Push2, Push2Colors, Push2Event};
use std::collections::HashMap;
use std::sync::mpsc;
use std::{error, time};
use tokio::fs as tokio_fs;
struct AppState {
    // mode: Mode,
    pad_files: HashMap<u8, PathBuf>,
    is_mute_enabled: bool,
    is_solo_enabled: bool,
    playback_volume: HashMap<u8, f64>,
    pitch_shift_semitones: HashMap<u8, f64>,
    active_recording_key: Option<u8>,
    selected_for_edit: Option<u8>,
    audio_cmd_tx: mpsc::Sender<AudioCommand>,
    is_delete_held: bool,
    is_select_held: bool,
}
// --- Color Constants for different states ---
const COLOR_OFF: u8 = Push2Colors::BLACK;
const COLOR_HAS_FILE: u8 = Push2Colors::BLUE_SKY;
const COLOR_RECORDING: u8 = Push2Colors::RED;
const COLOR_PLAYING: u8 = Push2Colors::PINK;
const COLOR_SELECTED: u8 = Push2Colors::PURPLE;
const BUTTON_LIGHT_ON: u8 = Push2Colors::GREEN_PALE;
const COLOR_VOLUME_BAR: Bgr565 = Bgr565::GREEN;
const COLOR_PITCH_BAR: Bgr565 = Bgr565::MAGENTA;
const COLOR_ENCODER_OUTLINE: Bgr565 = Bgr565::WHITE;
/// The display range for pitch, e.g., +/- 12 semitones.
const PITCH_RANGE_SEMITONES: f64 = 12.0;
#[tokio::main]
async fn main() -> Result<(), Box<dyn error::Error>> {
    env_logger::init();
    let (audio_tx, audio_rx) = mpsc::channel();
    std::thread::spawn(move || {
        println!("Audio capture thread started...");
        if let Err(e) = audio_capture::run_capture_loop(audio_rx) {
            eprintln!("Audio capture thread failed: {}", e);
        } else {
            println!("Audio capture thread exited cleanly.");
        }
    });
    // --- Config Loading ---
    let mut push2 = Push2::new()?;
    let audio_storage_path = get_audio_storage_path()?;
    println!("Audio storage path: {}", audio_storage_path.display());
    let mut app_state = AppState {
        // mode: Mode::Playback,
        pad_files: HashMap::new(),
        is_mute_enabled: false,
        is_solo_enabled: false,
        playback_volume: HashMap::new(),
        pitch_shift_semitones: HashMap::new(),
        active_recording_key: None,
        selected_for_edit: None,
        audio_cmd_tx: audio_tx,
        is_delete_held: false,
        is_select_held: false,
    };
    info!("\nConnection open. Soundboard example running.");
    info!(
        "Mute: {} | Solo: {}",
        app_state.is_mute_enabled, app_state.is_solo_enabled
    );
    for y in 0..8 {
        for x in 0..8 {
            let coord = PadCoord { x, y };
            let mut color = COLOR_OFF;
            if let Some(address) = push2.button_map.get_note_address(coord) {
                let file_name = format!("pad_{}_{}.wav", x, y);
                let file_path = audio_storage_path.join(file_name);
                if file_path.exists() {
                    color = COLOR_HAS_FILE;
                }
                app_state.pad_files.insert(address, file_path);
            }
            push2.set_pad_color(coord, color)?;
        }
    }
    // --- Main Loop ---
    loop {
        // -----------------------------------------------------------------
        // 1. EVENT POLLING: Handle all pending events
        // -----------------------------------------------------------------
        while let Some(event) = push2.poll_event() {
            debug!("Received event: {:?}", event);
            match event {
                Push2Event::PadPressed { coord, .. } => {
                    let Some(address) = push2.button_map.get_note_address(coord) else {
                        continue;
                    };
                    let Some(path) = app_state.pad_files.get(&address) else {
                        continue;
                    };
                    if app_state.is_delete_held {
                        info!("Delete held. Deleting sample on pad press.");
                        if let Some(path_buf) = app_state.pad_files.get(&address).cloned() {
                            if path_buf.exists() {
                                tokio::spawn(async move {
                                    match tokio_fs::remove_file(&path_buf).await {
                                        Ok(_) => {
                                            info!("...File {} deleted.", path_buf.display());
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "...Failed to delete file {}: {}",
                                                path_buf.display(),
                                                e
                                            );
                                        }
                                    }
                                });
                                app_state.pitch_shift_semitones.remove(&address);
                                app_state.playback_volume.remove(&address);
                                push2.set_pad_color(coord, COLOR_OFF)?;
                                // If this pad was the selected one, deselect it
                                if app_state.selected_for_edit == Some(address) {
                                    app_state.selected_for_edit = None;
                                }
                            }
                        }
                    } else if app_state.is_select_held {
                        // This is the logic from the old Mode::Edit
                        if !path.exists() {
                            continue;
                        }
                        if let Some(prev_selected_key) = app_state.selected_for_edit {
                            if prev_selected_key == address {
                                app_state.selected_for_edit = None;
                                push2.set_pad_color(coord, COLOR_HAS_FILE)?;
                            } else {
                                if let Some(old_coord) =
                                    push2.button_map.get_note(prev_selected_key)
                                {
                                    push2.set_pad_color(old_coord, COLOR_HAS_FILE)?;
                                }
                                app_state.selected_for_edit = Some(address);
                                push2.set_pad_color(coord, COLOR_SELECTED)?;
                            }
                        } else {
                            app_state.selected_for_edit = Some(address);
                            push2.set_pad_color(coord, COLOR_SELECTED)?;
                        }
                    } else {
                        if path.exists() {
                            push2.set_pad_color(coord, COLOR_PLAYING)?;
                        } else {
                            info!("START recording to {}", path.display());
                            let cmd = AudioCommand::Start(path.clone());
                            if let Err(e) = app_state.audio_cmd_tx.send(cmd) {
                                eprintln!("Failed to send START command: {}", e);
                            } else {
                                app_state.active_recording_key = Some(address);
                                push2.set_pad_color(coord, COLOR_RECORDING)?;
                            }
                        }
                    }
                }
                Push2Event::PadReleased { coord } => {
                    let Some(address) = push2.button_map.get_note_address(coord) else {
                        continue;
                    };
                    let Some(path) = app_state.pad_files.get(&address) else {
                        continue;
                    };
                    if app_state.is_delete_held || app_state.is_select_held {
                        // Reset color if it was a 'select' action, otherwise do nothing
                        if app_state.is_select_held {
                            if app_state.selected_for_edit == Some(address) {
                                push2.set_pad_color(coord, COLOR_SELECTED)?;
                            } else if path.exists() {
                                push2.set_pad_color(coord, COLOR_HAS_FILE)?;
                            } else {
                                push2.set_pad_color(coord, COLOR_OFF)?;
                            }
                        }
                        continue;
                    }
                    if app_state.active_recording_key == Some(address) {
                        info!("STOP recording.");
                        if let Err(e) = app_state.audio_cmd_tx.send(AudioCommand::Stop) {
                            eprintln!("Failed to send STOP command: {}", e);
                        }
                        app_state.active_recording_key = None;
                        push2.set_pad_color(coord, COLOR_HAS_FILE)?;
                    } else if path.exists() {
                        info!("Triggering playback for pad ({}, {}).", coord.x, coord.y);

                        // ‼️ --- START CHANGE: Auto-select pad on playback ---
                        // Store the previously selected key
                        let prev_selected_key = app_state.selected_for_edit;

                        // Set the new pad as selected
                        app_state.selected_for_edit = Some(address);

                        // If a *different* pad was selected before, reset its color
                        if let Some(prev_key) = prev_selected_key {
                            if prev_key != address {
                                if let Some(old_coord) = push2.button_map.get_note(prev_key) {
                                    // Reset old pad's color. We assume it has a file
                                    // because it was selectable.
                                    push2.set_pad_color(old_coord, COLOR_HAS_FILE)?;
                                }
                            }
                        }
                        // ‼️ --- END CHANGE ---

                        let pitch_shift = app_state
                            .pitch_shift_semitones
                            .get(&address)
                            .cloned()
                            .unwrap_or(0.0);
                        let path_clone = path.clone();
                        let sink_clone =
                            match (app_state.is_mute_enabled, app_state.is_solo_enabled) {
                                // Mute enabled, Solo enabled -> Default only
                                (true, true) => PlaybackSink::Default,
                                // Mute disabled, Solo enabled -> Both
                                (false, true) => PlaybackSink::Both,
                                // Mute disabled, Solo disabled -> Mixer only
                                (false, false) => PlaybackSink::Mixer,
                                // Mute enabled, Solo disabled -> None
                                (true, false) => PlaybackSink::None,
                            };
                        let volume_clone = app_state
                            .playback_volume
                            .get(&address)
                            .cloned()
                            .unwrap_or(1.0);
                        tokio::spawn(async move {
                            let mut temp_path: Option<PathBuf> = None;
                            let path_to_play = if pitch_shift.abs() > 0.01 {
                                let path_for_blocking = path_clone.clone();
                                match tokio::task::spawn_blocking(move || {
                                    audio_processor::create_pitched_copy_sync(
                                        &path_for_blocking,
                                        pitch_shift,
                                    )
                                })
                                .await
                                {
                                    Ok(Ok(new_path)) => {
                                        temp_path = Some(new_path.clone());
                                        new_path
                                    }
                                    Ok(Err(e)) => {
                                        eprintln!(
                                            "Failed to create pitched copy: {}. Playing original.",
                                            e
                                        );
                                        path_clone
                                    }
                                    Err(e) => {
                                        eprintln!("Task join error: {}. Playing original.", e);
                                        path_clone
                                    }
                                }
                            } else {
                                path_clone
                            };
                            if let Err(e) = audio_player::play_audio_file(
                                &path_to_play,
                                sink_clone,
                                volume_clone,
                            )
                            .await
                            {
                                eprintln!("Playback failed: {}", e);
                            }
                            if let Some(p) = temp_path {
                                if let Err(e) = tokio_fs::remove_file(&p).await {
                                    eprintln!(
                                        "Failed to clean up temp file {}: {}",
                                        p.display(),
                                        e
                                    );
                                }
                            }
                        });

                        // ‼️ Set color to SELECTED, not HAS_FILE
                        // push2.set_pad_color(coord, COLOR_HAS_FILE)?; // OLD
                        push2.set_pad_color(coord, COLOR_SELECTED)?; // NEW
                    } else {
                        push2.set_pad_color(coord, COLOR_OFF)?;
                    }
                }
                Push2Event::ButtonPressed { name, .. } => match name {
                    ControlName::Mute => {
                        app_state.is_mute_enabled = !app_state.is_mute_enabled;
                        let light = if app_state.is_mute_enabled {
                            BUTTON_LIGHT_ON
                        } else {
                            0
                        };
                        push2.set_button_light(name, light)?;
                        info!("Mute Toggled: {}", app_state.is_mute_enabled);
                    }
                    ControlName::Solo => {
                        app_state.is_solo_enabled = !app_state.is_solo_enabled;
                        let light = if app_state.is_solo_enabled {
                            BUTTON_LIGHT_ON
                        } else {
                            0
                        };
                        push2.set_button_light(name, light)?;
                        info!("Solo Toggled: {}", app_state.is_solo_enabled);
                    }
                    ControlName::Delete => {
                        app_state.is_delete_held = true;
                        push2.set_button_light(name, BUTTON_LIGHT_ON)?;
                    }
                    ControlName::Select => {
                        app_state.is_select_held = true;
                        push2.set_button_light(name, BUTTON_LIGHT_ON)?;
                    }
                    _ => {
                        debug!("--- Button {:?} PRESSED ---", name);
                        let is_modifier =
                            name == ControlName::Delete || name == ControlName::Select;
                        if !is_modifier {
                            push2.set_button_light(name, BUTTON_LIGHT_ON)?;
                        }
                    }
                },
                Push2Event::ButtonReleased { name } => {
                    debug!("--- Button {:?} RELEASED ---", name);
                    if name == ControlName::Delete {
                        app_state.is_delete_held = false;
                    }
                    if name == ControlName::Select {
                        app_state.is_select_held = false;
                    }
                    let is_mute_on = name == ControlName::Mute && app_state.is_mute_enabled;
                    let is_solo_on = name == ControlName::Solo && app_state.is_solo_enabled;
                    if !is_mute_on && !is_solo_on {
                        push2.set_button_light(name, 0)?;
                    }
                }
                Push2Event::EncoderTwisted {
                    name, raw_delta, ..
                } => {
                    let delta = if raw_delta > 64 {
                        -((128 - raw_delta) as i32)
                    } else {
                        raw_delta as i32
                    };
                    match name {
                        EncoderName::Track1 => {
                            if let Some(key) = app_state.selected_for_edit {
                                let current_volume =
                                    app_state.playback_volume.entry(key).or_insert(1.0);
                                *current_volume += delta as f64 * 0.01; // 1% per tick
                                *current_volume = current_volume.clamp(0.0, 1.5); // 0% to 150%
                                info!(
                                    "Set volume for selected pad to {:.0}%",
                                    *current_volume * 100.0
                                );
                            }
                        }
                        EncoderName::Track2 => {
                            if let Some(key) = app_state.selected_for_edit {
                                let current_pitch =
                                    app_state.pitch_shift_semitones.entry(key).or_insert(0.0);
                                *current_pitch += delta as f64 * 0.1; // 0.1 semitones per tick
                                *current_pitch = current_pitch
                                    .clamp(-PITCH_RANGE_SEMITONES, PITCH_RANGE_SEMITONES);
                                info!(
                                    "Set pitch for selected pad to {:.2} semitones",
                                    *current_pitch
                                );
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        // -----------------------------------------------------------------
        // -----------------------------------------------------------------
        // Clear the display buffer to black
        push2.display.clear(Bgr565::BLACK).unwrap(); // Infallible
        // Draw encoder bars only if a pad is selected
        if let Some(selected_key) = app_state.selected_for_edit {
            // --- Draw Volume Bar (Track 1, Index 0) ---
            let volume = app_state
                .playback_volume
                .get(&selected_key)
                .cloned()
                .unwrap_or(1.0);
            // Normalize volume (0.0 - 1.5) to a 0-127 i32 value
            let volume_norm = (volume / 1.5).clamp(0.0, 1.0);
            let volume_val = (volume_norm * 127.0) as i32;
            push2
                .display
                .draw_encoder_outline(0, COLOR_ENCODER_OUTLINE)
                .unwrap();
            push2
                .display
                .draw_encoder_bar(0, volume_val, COLOR_VOLUME_BAR)
                .unwrap();
            // --- Draw Pitch Bar (Track 2, Index 1) ---
            let pitch = app_state
                .pitch_shift_semitones
                .get(&selected_key)
                .cloned()
                .unwrap_or(0.0);
            // Normalize pitch (+/- PITCH_RANGE) to a 0-127 i32 value
            // Map [-12.0, 12.0] to [0.0, 1.0]
            let pitch_norm =
                ((pitch + PITCH_RANGE_SEMITONES) / (PITCH_RANGE_SEMITONES * 2.0)).clamp(0.0, 1.0);
            let pitch_val = (pitch_norm * 127.0) as i32;
            push2
                .display
                .draw_encoder_outline(1, COLOR_ENCODER_OUTLINE)
                .unwrap();
            push2
                .display
                .draw_encoder_bar(1, pitch_val, COLOR_PITCH_BAR)
                .unwrap();
        }
        // -----------------------------------------------------------------
        // -----------------------------------------------------------------
        if let Err(e) = push2.display.flush() {
            eprintln!("Failed to flush display: {}", e);
            // On a display error, we might want to break the loop
            break;
        }
        // -----------------------------------------------------------------
        // 4. SLEEP: Maintain a steady frame rate
        // -----------------------------------------------------------------
        tokio::time::sleep(time::Duration::from_millis(1000 / 60)).await;
    }
    Ok(())
}

