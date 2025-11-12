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


#[derive(Debug)]
pub enum AppCommand {
    FileSaved(PathBuf),
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
use embedded_graphics::{
    pixelcolor::Bgr565,
    prelude::*,
    primitives::{Line, Primitive, PrimitiveStyle},
};
use log::{debug, info, warn};
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
    waveform_cache: HashMap<u8, Option<Vec<(f32, f32)>>>,
    sample_start_point: HashMap<u8, f64>,
    sample_end_point: HashMap<u8, f64>,
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
const COLOR_WAVEFORM: Bgr565 = Bgr565::CYAN;
const COLOR_START_LINE: Bgr565 = Bgr565::GREEN;
const COLOR_STOP_LINE: Bgr565 = Bgr565::RED;
/// The display range for pitch, e.g., +/- 12 semitones.
const PITCH_RANGE_SEMITONES: f64 = 12.0;
// --- Waveform Display Constants ---
const WAVEFORM_Y_START: i32 = 0; // Top of display
const WAVEFORM_Y_END: i32 = 160; // Bottom of display
const WAVEFORM_X_START: i32 = 0;
const WAVEFORM_X_END: i32 = 960; // Full width of display
const WAVEFORM_WIDTH: i32 = WAVEFORM_X_END - WAVEFORM_X_START;
#[tokio::main]
async fn main() -> Result<(), Box<dyn error::Error>> {
    env_logger::init();
    let (audio_tx, audio_rx) = mpsc::channel();
    let (app_tx, app_rx) = mpsc::channel::<AppCommand>();

    std::thread::spawn(move || {
        println!("Audio capture thread started...");

        if let Err(e) = audio_capture::run_capture_loop(audio_rx, app_tx) {
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
        waveform_cache: HashMap::new(),
        sample_start_point: HashMap::new(),
        sample_end_point: HashMap::new(),
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
                                app_state.waveform_cache.remove(&address);
                                app_state.sample_start_point.remove(&address);
                                app_state.sample_end_point.remove(&address);
                                push2.set_pad_color(coord, COLOR_OFF)?;
                                // If this pad was the selected one, deselect it
                                if app_state.selected_for_edit == Some(address) {
                                    app_state.selected_for_edit = None;
                                }
                            }
                        }
                    } else if app_state.is_select_held {
                        // This is the logic for selecting a pad to edit
                        if !path.exists() {
                            continue;
                        }
                        if let Some(prev_selected_key) = app_state.selected_for_edit {
                            if prev_selected_key == address {
                                // Deselect if pressing the same pad
                                app_state.selected_for_edit = None;
                                push2.set_pad_color(coord, COLOR_HAS_FILE)?;
                            } else {
                                // Deselect old pad
                                if let Some(old_coord) =
                                    push2.button_map.get_note(prev_selected_key)
                                {
                                    push2.set_pad_color(old_coord, COLOR_HAS_FILE)?;
                                }
                                // Select new pad
                                app_state.selected_for_edit = Some(address);
                                push2.set_pad_color(coord, COLOR_SELECTED)?;
                            }
                        } else {
                            // No pad was selected, select this one
                            app_state.selected_for_edit = Some(address);
                            push2.set_pad_color(coord, COLOR_SELECTED)?;
                        }
                    } else {
                        // This is the default playback/record logic
                        if path.exists() {
                            // Pad has a file, set color to "playing" (will be reset on release)
                            push2.set_pad_color(coord, COLOR_PLAYING)?;
                        } else {
                            // Pad is empty, start recording
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
                    // If Select or Delete is held, we just reset the pad color
                    // (the action happened on press)
                    if app_state.is_delete_held || app_state.is_select_held {
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
                        // This was a playback trigger
                        info!("Triggering playback for pad ({}, {}).", coord.x, coord.y);
                        // --- Playback and Selection Logic ---
                        // Store the previously selected key
                        let prev_selected_key = app_state.selected_for_edit;
                        // Set the new pad as selected
                        app_state.selected_for_edit = Some(address);

                        // If a *different* pad was selected before, reset its color
                        if let Some(prev_key) = prev_selected_key {
                            if prev_key != address {
                                if let Some(old_coord) = push2.button_map.get_note(prev_key) {
                                    // Reset old pad's color (check if file exists)
                                    let old_color = if app_state
                                        .pad_files
                                        .get(&prev_key)
                                        .map_or(false, |p| p.exists())
                                    {
                                        COLOR_HAS_FILE
                                    } else {
                                        COLOR_OFF
                                    };
                                    push2.set_pad_color(old_coord, old_color)?;
                                }
                            }
                        }

                        // Get all playback parameters
                        let pitch_shift = app_state
                            .pitch_shift_semitones
                            .get(&address)
                            .cloned()
                            .unwrap_or(0.0);
                        let start_point = app_state
                            .sample_start_point
                            .get(&address)
                            .cloned()
                            .unwrap_or(0.0);
                        let end_point = app_state
                            .sample_end_point
                            .get(&address)
                            .cloned()
                            .unwrap_or(1.0);
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
                        // Spawn the playback task
                        tokio::spawn(async move {
                            let mut temp_path: Option<PathBuf> = None;
                            // (pitched AND trimmed)
                            let path_to_play = {
                                let path_for_blocking = path_clone.clone();
                                match tokio::task::spawn_blocking(move || {
                                    audio_processor::create_processed_copy_sync(
                                        &path_for_blocking,
                                        pitch_shift,
                                        start_point,
                                        end_point,
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
                                            "Failed to create processed copy: {}. Playing original.",
                                            e
                                        );
                                        path_clone
                                    }
                                    Err(e) => {
                                        eprintln!("Task join error: {}. Playing original.", e);
                                        path_clone
                                    }
                                }
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
                            // Clean up the temporary file
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
                        // Set pad color to selected (since it's now the active one)
                        push2.set_pad_color(coord, COLOR_SELECTED)?;
                    } else {
                        // Pad was empty and released (no recording happened)
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
                    // Normalize delta to a signed i32
                    let delta = if raw_delta > 64 {
                        -((128 - raw_delta) as i32)
                    } else {
                        raw_delta as i32
                    };
                    match name {
                        EncoderName::Track1 => {
                            // Volume Control
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
                            // Pitch Control
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
                        EncoderName::Track3 => {
                            // Start Point Control
                            if let Some(key) = app_state.selected_for_edit {
                                // Get the current end point to constrain the start point
                                let current_end =
                                    *app_state.sample_end_point.entry(key).or_insert(1.0);
                                let current_start =
                                    app_state.sample_start_point.entry(key).or_insert(0.0);
                                *current_start += delta as f64 * 0.005; // 0.5% per tick
                                // Clamp start from 0.0 up to the current end point
                                *current_start = current_start.clamp(0.0, current_end);
                                info!(
                                    "Set start point for selected pad to {:.2}%",
                                    *current_start * 100.0
                                );
                            }
                        }
                        EncoderName::Track4 => {
                            // End Point Control
                            if let Some(key) = app_state.selected_for_edit {
                                // Get the current start point to constrain the end point
                                let current_start =
                                    *app_state.sample_start_point.entry(key).or_insert(0.0);
                                let current_end =
                                    app_state.sample_end_point.entry(key).or_insert(1.0);
                                *current_end += delta as f64 * 0.005; // 0.5% per tick
                                // Clamp end from the current start point up to 1.0
                                *current_end = current_end.clamp(current_start, 1.0);
                                info!(
                                    "Set end point for selected pad to {:.2}%",
                                    *current_end * 100.0
                                );
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        



        while let Ok(app_event) = app_rx.try_recv() {
            match app_event {
                AppCommand::FileSaved(path) => {
                    info!("Main thread notified that {} was saved.", path.display());
                    
                    // Find the pad address associated with this path
                    let mut found_address = None;
                    for (addr, pad_path) in &app_state.pad_files {
                        if *pad_path == path {
                            found_address = Some(*addr);
                            break;
                        }
                    }

                    if let Some(address) = found_address {

                        app_state.waveform_cache.remove(&address);


                        let prev_selected_key = app_state.selected_for_edit;
                        app_state.selected_for_edit = Some(address);


                        if let Some(prev_key) = prev_selected_key {
                            if prev_key != address {
                                if let Some(old_coord) = push2.button_map.get_note(prev_key) {
                                    let old_color = if app_state.pad_files.get(&prev_key).map_or(false, |p| p.exists()) {
                                        COLOR_HAS_FILE
                                    } else {
                                        COLOR_OFF
                                    };
                                    // Don't panic on error, just log (or ignore)
                                    let _ = push2.set_pad_color(old_coord, old_color);
                                }
                            }
                        }
                        

                        if let Some(coord) = push2.button_map.get_note(address) {
                            let _ = push2.set_pad_color(coord, COLOR_SELECTED);
                        }
                    } else {
                        warn!("FileSaved event received for a path not in pad_files: {}", path.display());
                    }
                }
            }
        }

        // -----------------------------------------------------------------
        // 2. GUI DRAWING: Render the display
        // -----------------------------------------------------------------
        // Clear the display buffer to black
        push2.display.clear(Bgr565::BLACK).unwrap(); // Infallible
        // Draw waveform AND encoder bars only if a pad is selected
        if let Some(selected_key) = app_state.selected_for_edit {
            // This fixes the scope errors from before.
            // Get Volume (for Encoder 1)
            let volume = app_state
                .playback_volume
                .get(&selected_key)
                .cloned()
                .unwrap_or(1.0);
            // Get Pitch (for Encoder 2)
            let pitch = app_state
                .pitch_shift_semitones
                .get(&selected_key)
                .cloned()
                .unwrap_or(0.0);
            // Get Start Point (for Encoder 3)
            let start_pct = app_state
                .sample_start_point
                .get(&selected_key)
                .cloned()
                .unwrap_or(0.0) as f32; // Use f32 for drawing
            // Get End Point (for Encoder 4)
            let end_pct = app_state
                .sample_end_point
                .get(&selected_key)
                .cloned()
                .unwrap_or(1.0) as f32; // Use f32 for drawing
            // --- Load/Draw Waveform ---
            // Step 1: Check cache. If it's not there, load it.
            if !app_state.waveform_cache.contains_key(&selected_key) {
                warn!("Cache miss for waveform {}. Loading...", selected_key);
                let mut loaded_peaks: Option<Vec<(f32, f32)>> = None;
                if let Some(path) = app_state.pad_files.get(&selected_key) {
                    

                    if path.exists() {
                        // This is a blocking call! It will pause the main loop
                        // briefly on the first load of a sample.
                        match push2::gui::load_waveform_peaks(path, 960) {
                            // 960 = display width
                            Ok(peaks) => {
                                info!("...Successfully loaded waveform.");
                                loaded_peaks = Some(peaks);
                            }
                            Err(e) => {
                                warn!("Failed to load waveform for {}: {}", path.display(), e);
                                // Insert None to mark as "failed" and avoid retrying
                                loaded_peaks = None;
                            }
                        }
                    } else {

                        warn!("...Path {} does not exist, caching None.", path.display());
                        // File doesn't exist, cache None
                        loaded_peaks = None;
                    }
                }
                app_state.waveform_cache.insert(selected_key, loaded_peaks);
            }
            // Step 2: Draw the cached waveform (if it loaded successfully)
            if let Some(Some(peaks)) = app_state.waveform_cache.get(&selected_key) {
                if !peaks.is_empty() {
                    // draw_waveform_peaks is from the GuiApi trait
                    push2.display.draw_waveform_peaks(peaks, COLOR_WAVEFORM)?;
                }
                // Calculate X coordinates
                let start_x = WAVEFORM_X_START + (start_pct * WAVEFORM_WIDTH as f32).round() as i32;
                let end_x = WAVEFORM_X_START + (end_pct * WAVEFORM_WIDTH as f32).round() as i32;
                // Draw start line
                Line::new(
                    Point::new(start_x, WAVEFORM_Y_START),
                    Point::new(start_x, WAVEFORM_Y_END),
                )
                .into_styled(PrimitiveStyle::with_stroke(COLOR_START_LINE, 1))
                .draw(&mut push2.display)?;
                // Draw end line
                Line::new(
                    Point::new(end_x, WAVEFORM_Y_START),
                    Point::new(end_x, WAVEFORM_Y_END),
                )
                .into_styled(PrimitiveStyle::with_stroke(COLOR_STOP_LINE, 1))
                .draw(&mut push2.display)?;
            }
            // --- Draw Volume Bar (Track 1, Index 0) ---
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
            // --- Draw Start Bar (Track 3, Index 2) ---
            let start_val = (start_pct.clamp(0.0, 1.0) * 127.0) as i32;
            push2
                .display
                .draw_encoder_outline(2, COLOR_ENCODER_OUTLINE)
                .unwrap();
            push2
                .display
                .draw_encoder_bar(2, start_val, COLOR_START_LINE)
                .unwrap();
            // --- Draw End Bar (Track 4, Index 3) ---
            let end_val = (end_pct.clamp(0.0, 1.0) * 127.0) as i32;
            push2
                .display
                .draw_encoder_outline(3, COLOR_ENCODER_OUTLINE)
                .unwrap();
            push2
                .display
                .draw_encoder_bar(3, end_val, COLOR_STOP_LINE)
                .unwrap();
        }
        // -----------------------------------------------------------------
        // 3. FLUSH: Send the frame buffer to the display
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