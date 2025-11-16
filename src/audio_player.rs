use kira::{
    AudioManager,
    AudioManagerSettings,
    Easing,
    StartTime,
    Tween,
    backend::DefaultBackend,
    sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings},
};
use log::debug;
use std::{collections::HashMap, sync::mpsc::Receiver, time::Duration};

#[derive(Debug)]
pub struct KiraPlayRequest {
    pub pad_key: u8,
    pub sound_data: StaticSoundData,
    pub settings: StaticSoundSettings,
}

#[derive(Debug)]
pub enum KiraCommand {
    Play(KiraPlayRequest),
    Stop(u8),                 // Stop sound for a specific pad key
    SetPlaybackRate(u8, f64),
}

/// This is kept from the original to maintain Mute/Solo logic.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PlaybackSink {
    Default,
    Mixer,
    Both,
    None,
}

pub fn run_kira_loop(rx: Receiver<KiraCommand>) -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?;
    // Track all actively playing sound handles
    let mut active_handles: HashMap<u8, StaticSoundHandle> = HashMap::new();

    // This loop blocks on `rx.recv()`, waiting for commands from the main thread.
    for command in rx {
        match command {
            KiraCommand::Play(req) => {
                debug!("Kira thread received Play command for key {}", req.pad_key);
                // If a sound is already playing for this pad, stop it first.
                if let Some(mut old_handle) = active_handles.remove(&req.pad_key) {
                    // Use a tiny fade-out to avoid clicks
                    let tween = Tween {
                        start_time: StartTime::Immediate,
                        duration: Duration::from_millis(10),
                        easing: Easing::Linear,
                    };
                    let _ = old_handle.stop(tween);
                }
                // Play the new sound
                match manager.play(req.sound_data.with_settings(req.settings)) {
                    Ok(handle) => {
                        active_handles.insert(req.pad_key, handle);
                    }
                    Err(e) => {
                        eprintln!("Failed to play sound: {}", e);
                    }
                }
            }
            KiraCommand::Stop(key) => {
                debug!("Kira thread received Stop command for key {}", key);
                // Stop the sound for the given pad key
                if let Some(mut handle) = active_handles.remove(&key) {
                    // Use a tiny fade-out to avoid clicks
                    let tween = Tween {
                        start_time: StartTime::Immediate,
                        duration: Duration::from_millis(10),
                        easing: Easing::Linear,
                    };
                    let _ = handle.stop(tween);
                }
            }

            KiraCommand::SetPlaybackRate(key, rate) => {
                debug!(
                    "Kira thread received SetPlaybackRate for key {} to {}",
                    key, rate
                );
                // Find the active handle for this key
                if let Some(handle) = active_handles.get_mut(&key) {
                    // Use a short tween to smooth the transition and avoid clicks
                    let tween = Tween {
                        start_time: StartTime::Immediate,
                        duration: Duration::from_millis(10),
                        easing: Easing::Linear,
                    };
                    // Set the playback rate on the running sound
                    handle.set_playback_rate(rate, tween)
                }
            }
        }
    }

    println!("Kira command channel closed. Exiting audio loop.");
    Ok(())
}