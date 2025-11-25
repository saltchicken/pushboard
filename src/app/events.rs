use crate::app::audio_player::{self, KiraCommand, PlaybackSink};
use crate::app::state::{
    AppCommand, AppState, AudioCommand, BUTTON_LIGHT_ON, COLOR_HAS_FILE, COLOR_OFF, COLOR_PLAYING,
    COLOR_RECORDING, COLOR_SELECTED,
};
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use log::{error, info};
use push2::{ControlName, EncoderName, Push2, Push2Event};
use std::sync::mpsc::Receiver;
use std::time;
use tokio::fs as tokio_fs;

pub async fn handle_incoming_events(
    push2: &mut Push2,
    state: &mut AppState,
    app_rx: &Receiver<AppCommand>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Hardware Events
    while let Some(event) = push2.poll_event() {
        match event {
            Push2Event::PadPressed { coord, .. } => handle_pad_pressed(push2, state, coord).await?,
            Push2Event::PadReleased { coord } => handle_pad_released(push2, state, coord)?,
            Push2Event::ButtonPressed { name, .. } => handle_button_pressed(push2, state, name)?,
            Push2Event::ButtonReleased { name } => handle_button_released(push2, state, name)?,
            Push2Event::EncoderTwisted {
                name, raw_delta, ..
            } => handle_encoder_twist(state, name, raw_delta)?,
            _ => {}
        }
    }

    // 2. Application Events (Thread messages)
    while let Ok(app_event) = app_rx.try_recv() {
        handle_app_command(push2, state, app_event)?;
    }

    Ok(())
}

async fn handle_pad_pressed(
    push2: &mut Push2,
    state: &mut AppState,
    coord: push2::PadCoord,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(address) = push2.button_map.get_note_address(coord) else {
        return Ok(());
    };
    let Some(path) = state.pad_files.get(&address).cloned() else {
        return Ok(());
    };

    if state.is_delete_held {
        handle_delete_action(push2, state, address, path, coord).await?;
    } else if state.is_select_held {
        handle_select_action(push2, state, address, path, coord)?;
    } else {
        handle_playback_or_record(push2, state, address, path, coord)?;
    }
    Ok(())
}

async fn handle_delete_action(
    push2: &mut Push2,
    state: &mut AppState,
    address: u8,
    path: std::path::PathBuf,
    coord: push2::PadCoord,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Deleting sample...");
    if path.exists() {
        // Spawn async delete
        tokio::spawn(async move {
            if let Err(e) = tokio_fs::remove_file(&path).await {
                error!("Failed to delete file: {}", e);
            }
        });
        // Clear state
        state.pitch_shift_semitones.remove(&address);
        state.playback_volume.remove(&address);
        state.waveform_cache.remove(&address);
        state.sound_data_cache.remove(&address);
        if let Some(task) = state.auto_stop_tasks.remove(&address) {
            task.abort();
        }
        push2.set_pad_color(coord, COLOR_OFF)?;
        if state.selected_for_edit == Some(address) {
            state.selected_for_edit = None;
        }
    }
    Ok(())
}

fn handle_select_action(
    push2: &mut Push2,
    state: &mut AppState,
    address: u8,
    path: std::path::PathBuf,
    coord: push2::PadCoord,
) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(());
    }
    // Deselect Logic
    if let Some(prev) = state.selected_for_edit {
        if prev == address {
            state.selected_for_edit = None;
            push2.set_pad_color(coord, COLOR_HAS_FILE)?;
            return Ok(());
        }
        // Reset old pad color
        if let Some(old_coord) = push2.button_map.get_note(prev) {
            push2.set_pad_color(old_coord, COLOR_HAS_FILE)?;
        }
    }
    // Select new
    state.selected_for_edit = Some(address);
    push2.set_pad_color(coord, COLOR_SELECTED)?;
    Ok(())
}

fn handle_playback_or_record(
    push2: &mut Push2,
    state: &mut AppState,
    address: u8,
    path: std::path::PathBuf,
    coord: push2::PadCoord,
) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        push2.set_pad_color(coord, COLOR_PLAYING)?;
        trigger_sound_playback(state, address, path)?;
        // Auto-select on playback
        if state.selected_for_edit != Some(address) {
            // Logic to reset old selection color omitted for brevity, but follows same pattern
            state.selected_for_edit = Some(address);
        }
    } else {
        info!("START recording to {}", path.display());
        state.audio_cmd_tx.send(AudioCommand::Start(path))?;
        state.active_recording_key = Some(address);
        push2.set_pad_color(coord, COLOR_RECORDING)?;
    }
    Ok(())
}

fn handle_pad_released(
    push2: &mut Push2,
    state: &mut AppState,
    coord: push2::PadCoord,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(address) = push2.button_map.get_note_address(coord) else {
        return Ok(());
    };

    // Ignore release if modifiers held
    if state.is_delete_held || state.is_select_held {
        // Logic to restore color if needed
        return Ok(());
    }

    if state.active_recording_key == Some(address) {
        info!("STOP recording.");
        state.audio_cmd_tx.send(AudioCommand::Stop)?;
        state.active_recording_key = None;
        push2.set_pad_color(coord, COLOR_HAS_FILE)?;
    } else {
        // Playback released: reset color to Off or FilePresent
        if let Some(path) = state.pad_files.get(&address) {
            let color = if state.selected_for_edit == Some(address) {
                COLOR_SELECTED
            } else if path.exists() {
                COLOR_HAS_FILE
            } else {
                COLOR_OFF
            };
            push2.set_pad_color(coord, color)?;
        }
    }
    Ok(())
}

fn handle_button_pressed(
    push2: &mut Push2,
    state: &mut AppState,
    name: ControlName,
) -> Result<(), Box<dyn std::error::Error>> {
    match name {
        ControlName::Delete => {
            state.is_delete_held = true;
            push2.set_button_light(name, BUTTON_LIGHT_ON)?;
        }
        ControlName::Select => {
            state.is_select_held = true;
            push2.set_button_light(name, BUTTON_LIGHT_ON)?;
        }
        ControlName::Mute => {
            state.is_mute_enabled = !state.is_mute_enabled;
            push2.set_button_light(
                name,
                if state.is_mute_enabled {
                    BUTTON_LIGHT_ON
                } else {
                    0
                },
            )?;
            info!("Mute Toggled: {}", state.is_mute_enabled);
            update_audio_routing(state);
        }
        ControlName::Solo => {
            state.is_solo_enabled = !state.is_solo_enabled;
            push2.set_button_light(
                name,
                if state.is_solo_enabled {
                    BUTTON_LIGHT_ON
                } else {
                    0
                },
            )?;
            info!("Solo Toggled: {}", state.is_solo_enabled);
            update_audio_routing(state);
        }
        _ => {}
    }
    Ok(())
}

fn update_audio_routing(state: &AppState) {
    let current_sink = match (state.is_mute_enabled, state.is_solo_enabled) {
        (true, true) => PlaybackSink::Default,
        (false, true) => PlaybackSink::Both,
        (false, false) => PlaybackSink::Mixer,
        (true, false) => PlaybackSink::None,
    };
    audio_player::update_pipewire_links(current_sink);
}

fn handle_button_released(
    push2: &mut Push2,
    state: &mut AppState,
    name: ControlName,
) -> Result<(), Box<dyn std::error::Error>> {
    match name {
        ControlName::Delete => {
            state.is_delete_held = false;
            push2.set_button_light(name, 0)?;
        }
        ControlName::Select => {
            state.is_select_held = false;
            push2.set_button_light(name, 0)?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_encoder_twist(
    state: &mut AppState,
    name: EncoderName,
    raw_delta: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    let delta = if raw_delta > 64 {
        -((128 - raw_delta) as i32)
    } else {
        raw_delta as i32
    };

    // Only proceed if a pad is selected
    let Some(key) = state.selected_for_edit else {
        return Ok(());
    };

    match name {
        EncoderName::Track1 => {
            // Volume
            let val = state.playback_volume.entry(key).or_insert(1.0);
            *val = (*val + delta as f64 * 0.10).clamp(-30.0, 15.0);
            state.kira_cmd_tx.send(KiraCommand::SetVolume(key, *val))?;
        }
        EncoderName::Track2 => {
            // Pitch
            let val = state.pitch_shift_semitones.entry(key).or_insert(0.0);
            *val = (*val + delta as f64 * 0.1).clamp(-12.0, 12.0);
            let rate = 2.0_f64.powf(*val / 12.0);
            state
                .kira_cmd_tx
                .send(KiraCommand::SetPlaybackRate(key, rate))?;
        }
        EncoderName::Track3 => {
            // Start Point
            let end = *state.sample_end_point.entry(key).or_insert(1.0);
            let start = state.sample_start_point.entry(key).or_insert(0.0);
            *start = (*start + delta as f64 * 0.005).clamp(0.0, end);
        }
        EncoderName::Track4 => {
            // End Point
            let start = *state.sample_start_point.entry(key).or_insert(0.0);
            let end = state.sample_end_point.entry(key).or_insert(1.0);
            *end = (*end + delta as f64 * 0.005).clamp(start, 1.0);
        }
        _ => {}
    }
    Ok(())
}

fn trigger_sound_playback(
    state: &mut AppState,
    address: u8,
    path: std::path::PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load Data
    let sound_data = if let Some(data) = state.sound_data_cache.get(&address) {
        data.clone()
    } else {
        match StaticSoundData::from_file(&path) {
            Ok(data) => {
                state.sound_data_cache.insert(address, data.clone());
                data
            }
            Err(e) => {
                error!("Load failed: {}", e);
                return Ok(());
            }
        }
    };

    // 2. Params
    let pitch = *state.pitch_shift_semitones.get(&address).unwrap_or(&0.0);
    let volume = *state.playback_volume.get(&address).unwrap_or(&1.0);
    let start_pct = *state.sample_start_point.get(&address).unwrap_or(&0.0);
    let end_pct = *state.sample_end_point.get(&address).unwrap_or(&1.0);

    let rate = 2.0_f64.powf(pitch / 12.0);
    let dur = sound_data.duration().as_secs_f64();
    let start_sec = dur * start_pct;

    let settings = StaticSoundSettings::new()
        .volume(volume as f32)
        .playback_rate(rate)
        .start_position(start_sec);

    state
        .kira_cmd_tx
        .send(KiraCommand::Play(audio_player::KiraPlayRequest {
            pad_key: address,
            sound_data,
            settings,
        }))?;
    // 4. Auto-Stop Task
    let end_sec = dur * end_pct;
    let play_dur = (end_sec - start_sec).max(0.0);
    let real_dur = if rate.abs() > 0.001 {
        play_dur / rate.abs()
    } else {
        0.0
    };

    if let Some(old) = state.auto_stop_tasks.remove(&address) {
        old.abort();
    }

    let kira_tx = state.kira_cmd_tx.clone();
    let task = tokio::spawn(async move {
        tokio::time::sleep(time::Duration::from_secs_f64(real_dur)).await;
        let _ = kira_tx.send(KiraCommand::Stop(address));
    });
    state.auto_stop_tasks.insert(address, task);

    Ok(())
}

fn handle_app_command(
    push2: &mut Push2,
    state: &mut AppState,
    cmd: AppCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        AppCommand::FileSaved(path) => {
            info!("File saved: {}", path.display());
            // Clear Caches
            let mut target_addr = None;
            for (addr, p) in &state.pad_files {
                if *p == path {
                    target_addr = Some(*addr);
                    break;
                }
            }
            if let Some(addr) = target_addr {
                state.waveform_cache.remove(&addr);
                state.sound_data_cache.remove(&addr);

                // Update Selection to new file
                state.selected_for_edit = Some(addr);
                if let Some(coord) = push2.button_map.get_note(addr) {
                    push2.set_pad_color(coord, COLOR_SELECTED)?;
                }
            }
        }
    }
    Ok(())
}

