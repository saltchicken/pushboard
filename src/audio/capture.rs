// ‼️ Moved from src/app/audio_capture.rs
use crate::app::state::{AppCommand, AudioCommand};
use hound::{SampleFormat, WavSpec, WavWriter};
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;
use spa::pod::Pod;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::fs;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    mpsc::{Receiver, Sender},
};
use std::thread;

const PRE_BUFFER_SECONDS: u32 = 1;

#[derive(Debug, PartialEq, Clone)]
enum State {
    Listening,
    Recording(PathBuf),
}

struct UserData {
    format: Option<spa::param::audio::AudioInfoRaw>,
    state: State,
    buffer: VecDeque<f32>,
    pre_buffer_max_samples: usize,
}

fn save_recording_from_buffer(
    buffer: VecDeque<f32>,
    format: &spa::param::audio::AudioInfoRaw,
    filename: &Path,
) {
    if buffer.is_empty() {
        return;
    }
    if let Some(parent) = filename.parent() {
        if !parent.exists() {
            let _ = fs::create_dir_all(parent);
        }
    }

    let spec = WavSpec {
        channels: format.channels() as u16,
        sample_rate: format.rate(),
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    if let Ok(mut writer) = WavWriter::create(filename, spec) {
        for &sample in &buffer {
            let _ = writer.write_sample(sample);
        }
        let _ = writer.finalize();
    }
}

fn handle_audio_commands(
    rx: Receiver<AudioCommand>,
    data: Arc<Mutex<UserData>>,
    app_tx: Sender<AppCommand>,
) {
    for command in rx {
        let mut save_data: Option<(VecDeque<f32>, spa::param::audio::AudioInfoRaw, PathBuf)> = None;
        {
            let mut user_data = data.lock().unwrap();
            match command {
                AudioCommand::Start(path) => {
                    if user_data.format.is_some() {
                        if let State::Listening = user_data.state {
                            user_data.state = State::Recording(path);
                        }
                    }
                }
                AudioCommand::Stop => {
                    let old_state = std::mem::replace(&mut user_data.state, State::Listening);
                    if let State::Recording(save_path) = old_state {
                        let buffer_to_save = std::mem::take(&mut user_data.buffer);
                        if let Some(fmt) = user_data.format {
                            save_data = Some((buffer_to_save, fmt, save_path));
                        }
                    }
                }
            }
        }

        if let Some((buffer, format, path)) = save_data {
            save_recording_from_buffer(buffer, &format, &path);
            let _ = app_tx.send(AppCommand::FileSaved(path));
        }
    }
}

pub fn run_capture_loop(
    rx: Receiver<AudioCommand>,
    app_tx: Sender<AppCommand>,
) -> Result<(), pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let data = Arc::new(Mutex::new(UserData {
        format: None,
        state: State::Listening,
        buffer: VecDeque::new(),
        pre_buffer_max_samples: 0,
    }));

    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::STREAM_CAPTURE_SINK => "true",
    };

    let stream = pw::stream::StreamBox::new(&core, "audio-capture", props)?;

    let _listener = stream
        .add_local_listener_with_user_data(data.clone())
        .param_changed(|_, user_data_arc, id, param| {
            if let Some(param) = param {
                if id == pw::spa::param::ParamType::Format.as_raw() {
                    if let Ok((MediaType::Audio, MediaSubtype::Raw)) =
                        format_utils::parse_format(param)
                    {
                        let mut user_data = user_data_arc.lock().unwrap();
                        let mut info = spa::param::audio::AudioInfoRaw::new();
                        if info.parse(param).is_ok() {
                            user_data.format = Some(info);
                            user_data.pre_buffer_max_samples =
                                (info.rate() * info.channels() * PRE_BUFFER_SECONDS) as usize;
                        }
                    }
                }
            }
        })
        .process(|stream, user_data_arc| {
            const GAIN_FACTOR: f32 = 2.0;
            let mut user_data = user_data_arc.lock().unwrap();
            if user_data.format.is_none() {
                return;
            }

            match stream.dequeue_buffer() {
                None => {}
                Some(mut buffer) => {
                    let datas = buffer.datas_mut();
                    if !datas.is_empty() {
                        let data = &mut datas[0];
                        let n_samples = data.chunk().size() / (mem::size_of::<f32>() as u32);
                        if let Some(samples) = data.data() {
                            let mut all_samples = Vec::with_capacity(n_samples as usize);
                            for n in 0..(n_samples as usize) {
                                let start = n * mem::size_of::<f32>();
                                let end = start + mem::size_of::<f32>();
                                let chan = &samples[start..end];
                                let sample = f32::from_le_bytes(chan.try_into().unwrap());
                                let amplified_sample = sample * GAIN_FACTOR;
                                all_samples.push(amplified_sample.clamp(-1.0, 1.0));
                            }
                            user_data.buffer.extend(&all_samples);

                            if user_data.state == State::Listening {
                                let max_samples = user_data.pre_buffer_max_samples;
                                if max_samples > 0 {
                                    let current_len = user_data.buffer.len();
                                    if current_len > max_samples {
                                        let samples_to_remove = current_len - max_samples;
                                        user_data.buffer.drain(..samples_to_remove);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
        .register()?;

    // Connect stream
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .unwrap()
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).unwrap()];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    let ipc_data = data.clone();
    thread::spawn(move || {
        handle_audio_commands(rx, ipc_data, app_tx);
    });

    mainloop.run();
    Ok(())
}
