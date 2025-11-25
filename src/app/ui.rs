use crate::app::state::AppState;
use embedded_graphics::{
    pixelcolor::Bgr565,
    prelude::*,
    primitives::{Line, Primitive, PrimitiveStyle},
};
use log::warn;
use push2::{GuiApi, Push2};

// Constants moved here
const WAVEFORM_Y_START: i32 = 0;
const WAVEFORM_Y_END: i32 = 160;
const WAVEFORM_X_START: i32 = 0;
const WAVEFORM_X_END: i32 = 960;
const WAVEFORM_WIDTH: i32 = WAVEFORM_X_END - WAVEFORM_X_START;
const COLOR_WAVEFORM: Bgr565 = Bgr565::CYAN;
const COLOR_START_LINE: Bgr565 = Bgr565::GREEN;
const COLOR_STOP_LINE: Bgr565 = Bgr565::RED;
const COLOR_ENCODER_OUTLINE: Bgr565 = Bgr565::WHITE;
const COLOR_VOLUME_BAR: Bgr565 = Bgr565::GREEN;
const COLOR_PITCH_BAR: Bgr565 = Bgr565::MAGENTA;

pub fn draw_screen(
    push2: &mut Push2,
    state: &mut AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    push2.display.clear(Bgr565::BLACK).unwrap();

    if let Some(key) = state.selected_for_edit {
        draw_waveform(push2, state, key)?;
        draw_encoders(push2, state, key)?;
    }

    push2.display.flush()?;
    Ok(())
}

fn draw_waveform(
    push2: &mut Push2,
    state: &mut AppState,
    key: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load/Cache
    if !state.waveform_cache.contains_key(&key) {
        let mut peaks = None;
        if let Some(path) = state.pad_files.get(&key) {
            if path.exists() {
                match push2::gui::load_waveform_peaks(path, 960) {
                    Ok(p) => peaks = Some(p),
                    Err(e) => warn!("Waveform load error: {}", e),
                }
            }
        }
        state.waveform_cache.insert(key, peaks);
    }

    // 2. Draw Peaks
    if let Some(Some(peaks)) = state.waveform_cache.get(&key) {
        push2.display.draw_waveform_peaks(peaks, COLOR_WAVEFORM)?;

        // 3. Draw Lines
        let start_pct = *state.sample_start_point.get(&key).unwrap_or(&0.0) as f32;
        let end_pct = *state.sample_end_point.get(&key).unwrap_or(&1.0) as f32;

        let start_x = WAVEFORM_X_START + (start_pct * WAVEFORM_WIDTH as f32).round() as i32;
        let end_x = WAVEFORM_X_START + (end_pct * WAVEFORM_WIDTH as f32).round() as i32;

        draw_vertical_line(push2, start_x, COLOR_START_LINE)?;
        draw_vertical_line(push2, end_x, COLOR_STOP_LINE)?;
    }
    Ok(())
}

fn draw_vertical_line(
    push2: &mut Push2,
    x: i32,
    color: Bgr565,
) -> Result<(), Box<dyn std::error::Error>> {
    Line::new(
        Point::new(x, WAVEFORM_Y_START),
        Point::new(x, WAVEFORM_Y_END),
    )
    .into_styled(PrimitiveStyle::with_stroke(color, 1))
    .draw(&mut push2.display)?;
    Ok(())
}

fn draw_encoders(
    push2: &mut Push2,
    state: &mut AppState,
    key: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    // Volume (Track 1)
    let vol = *state.playback_volume.get(&key).unwrap_or(&1.0);
    let vol_norm = ((vol - -30.0) / (15.0 - -30.0)).clamp(0.0, 1.0);
    draw_single_encoder(push2, 0, vol_norm, COLOR_VOLUME_BAR)?;

    // Pitch (Track 2)
    let pitch = *state.pitch_shift_semitones.get(&key).unwrap_or(&0.0);
    let pitch_norm = ((pitch + 12.0) / 24.0).clamp(0.0, 1.0);
    draw_single_encoder(push2, 1, pitch_norm, COLOR_PITCH_BAR)?;

    // Start (Track 3)
    let start = *state.sample_start_point.get(&key).unwrap_or(&0.0);
    draw_single_encoder(push2, 2, start, COLOR_START_LINE)?;

    // End (Track 4)
    let end = *state.sample_end_point.get(&key).unwrap_or(&1.0);
    draw_single_encoder(push2, 3, end, COLOR_STOP_LINE)?;

    Ok(())
}

fn draw_single_encoder(
    push2: &mut Push2,
    index: usize,
    normalized_value: f64,
    color: Bgr565,
) -> Result<(), Box<dyn std::error::Error>> {
    let val = (normalized_value * 127.0) as i32;
    push2
        .display
        .draw_encoder_outline(index as u8, COLOR_ENCODER_OUTLINE)?;
    push2.display.draw_encoder_bar(index as u8, val, color)?;
    Ok(())
}