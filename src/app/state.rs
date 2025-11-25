use crate::app::audio_player::KiraCommand;
use kira::sound::static_sound::StaticSoundData;
use log::info;
use push2::{PadCoord, Push2, Push2Colors};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use tokio::task::JoinHandle;


pub const COLOR_OFF: u8 = Push2Colors::BLACK;
pub const COLOR_HAS_FILE: u8 = Push2Colors::BLUE_SKY;
pub const COLOR_RECORDING: u8 = Push2Colors::RED;
pub const COLOR_PLAYING: u8 = Push2Colors::PINK;
pub const COLOR_SELECTED: u8 = Push2Colors::PURPLE;
pub const BUTTON_LIGHT_ON: u8 = Push2Colors::GREEN_PALE;

#[derive(Serialize, Deserialize, Debug)]
pub enum AudioCommand {
    Start(PathBuf),
    Stop,
}

#[derive(Debug)]
pub enum AppCommand {
    FileSaved(PathBuf),
}

pub struct AppState {
    pub pad_files: HashMap<u8, PathBuf>,
    pub is_mute_enabled: bool,
    pub is_solo_enabled: bool,
    pub playback_volume: HashMap<u8, f64>,
    pub pitch_shift_semitones: HashMap<u8, f64>,
    pub active_recording_key: Option<u8>,
    pub selected_for_edit: Option<u8>,
    pub audio_cmd_tx: mpsc::Sender<AudioCommand>,
    pub is_delete_held: bool,
    pub is_select_held: bool,
    pub waveform_cache: HashMap<u8, Option<Vec<(f32, f32)>>>,
    pub sample_start_point: HashMap<u8, f64>,
    pub sample_end_point: HashMap<u8, f64>,
    pub kira_cmd_tx: mpsc::Sender<KiraCommand>,
    pub sound_data_cache: HashMap<u8, StaticSoundData>,
    pub auto_stop_tasks: HashMap<u8, JoinHandle<()>>,
    pub audio_storage_path: PathBuf,
}

impl AppState {

    pub fn new(
        audio_cmd_tx: mpsc::Sender<AudioCommand>,
        kira_cmd_tx: mpsc::Sender<KiraCommand>,
    ) -> std::io::Result<Self> {
        let audio_storage_path = get_audio_storage_path()?;
        info!("Audio storage path: {}", audio_storage_path.display());

        Ok(Self {
            pad_files: HashMap::new(),
            is_mute_enabled: true,
            is_solo_enabled: true,
            playback_volume: HashMap::new(),
            pitch_shift_semitones: HashMap::new(),
            active_recording_key: None,
            selected_for_edit: None,
            audio_cmd_tx,
            is_delete_held: false,
            is_select_held: false,
            waveform_cache: HashMap::new(),
            sample_start_point: HashMap::new(),
            sample_end_point: HashMap::new(),
            kira_cmd_tx,
            sound_data_cache: HashMap::new(),
            auto_stop_tasks: HashMap::new(),
            audio_storage_path,
        })
    }


    pub fn update_pad_lights(
        &mut self,
        push2: &mut Push2,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for y in 0..8 {
            for x in 0..8 {
                let coord = PadCoord { x, y };
                let mut color = COLOR_OFF;
                if let Some(address) = push2.button_map.get_note_address(coord) {
                    let file_name = format!("pad_{}_{}.wav", x, y);
                    let file_path = self.audio_storage_path.join(file_name);
                    if file_path.exists() {
                        color = COLOR_HAS_FILE;
                    }
                    self.pad_files.insert(address, file_path);
                }
                push2.set_pad_color(coord, color)?;
            }
        }
        Ok(())
    }
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