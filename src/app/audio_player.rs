use kira::{
    AudioManager, AudioManagerSettings, Easing, StartTime, Tween,
    backend::DefaultBackend,
    sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings},
};
use log::{error, info};
use std::{collections::HashMap, process::Command, sync::mpsc::Receiver, time::Duration};


const LINK_APP_MIXER: &str = "alsa_playback.pushboard";
const LINK_TARGET_MIXER: &str = "MyMixer";
const LINK_APP_DEFAULT: &str = "alsa_playback.pushboard";
const LINK_TARGET_DEFAULT: &str = "alsa_output.usb-Generic_USB_Audio-00.HiFi__Speaker__sink";


#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PlaybackSink {
    Default,
    Mixer,
    Both,
    None,
}

#[derive(Debug)]
pub struct KiraPlayRequest {
    pub pad_key: u8,
    pub sound_data: StaticSoundData,
    pub settings: StaticSoundSettings,
}

#[derive(Debug)]
pub enum KiraCommand {
    Play(KiraPlayRequest),
    Stop(u8),
    SetPlaybackRate(u8, f64),
    SetVolume(u8, f64),
}

pub fn run_kira_loop(rx: Receiver<KiraCommand>) -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?;
    let mut active_handles: HashMap<u8, StaticSoundHandle> = HashMap::new();


    // update_pipewire_links(PlaybackSink::Mixer);

    for command in rx {
        match command {
            KiraCommand::Play(req) => {
                stop_sound_if_playing(&mut active_handles, req.pad_key);
                match manager.play(req.sound_data.with_settings(req.settings)) {
                    Ok(handle) => {
                        active_handles.insert(req.pad_key, handle);
                    }
                    Err(e) => error!("Failed to play: {}", e),
                }
            }
            KiraCommand::Stop(key) => {
                stop_sound_if_playing(&mut active_handles, key);
            }
            KiraCommand::SetPlaybackRate(key, rate) => {
                if let Some(handle) = active_handles.get_mut(&key) {
                    let _ = handle.set_playback_rate(rate, fast_tween());
                }
            }
            KiraCommand::SetVolume(key, vol) => {
                if let Some(handle) = active_handles.get_mut(&key) {
                    let _ = handle.set_volume(vol as f32, fast_tween());
                }
            }
        }
    }
    Ok(())
}


pub fn update_pipewire_links(sink: PlaybackSink) {
    let run_link = |connect: bool, output: &str, input: &str| {
        let mut cmd = Command::new("pw-link");
        if !connect {
            cmd.arg("-d");
        }
        cmd.arg(output).arg(input);
        // Ignore errors as links might already exist/not exist
        let _ = cmd.output();
    };

    match sink {
        PlaybackSink::Default => {
            run_link(true, LINK_APP_DEFAULT, LINK_TARGET_DEFAULT);
            run_link(false, LINK_APP_MIXER, LINK_TARGET_MIXER);
            info!("Audio Routing: Default Speaker Only");
        }
        PlaybackSink::Mixer => {
            run_link(false, LINK_APP_DEFAULT, LINK_TARGET_DEFAULT);
            run_link(true, LINK_APP_MIXER, LINK_TARGET_MIXER);
            info!("Audio Routing: MyMixer Only");
        }
        PlaybackSink::Both => {
            run_link(true, LINK_APP_DEFAULT, LINK_TARGET_DEFAULT);
            run_link(true, LINK_APP_MIXER, LINK_TARGET_MIXER);
            info!("Audio Routing: Both");
        }
        PlaybackSink::None => {
            run_link(false, LINK_APP_DEFAULT, LINK_TARGET_DEFAULT);
            run_link(false, LINK_APP_MIXER, LINK_TARGET_MIXER);
            info!("Audio Routing: Muted (None)");
        }
    }
}

fn stop_sound_if_playing(handles: &mut HashMap<u8, StaticSoundHandle>, key: u8) {
    if let Some(mut handle) = handles.remove(&key) {
        let _ = handle.stop(fast_tween());
    }
}

fn fast_tween() -> Tween {
    Tween {
        start_time: StartTime::Immediate,
        duration: Duration::from_millis(10),
        easing: Easing::Linear,
    }
}