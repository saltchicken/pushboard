# Pushboard

[![Rust](https://img.shields.io/badge/built_with-Rust-d66c2c.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20PipeWire-important.svg)]()

**Pushboard** is a hardware-integrated sampler and soundboard designed specifically for the **Ableton Push 2** on Linux. 

It leverages the low-latency capabilities of **PipeWire** to capture system audio on the fly, map it to pads, and manipulate playback in real-time directly from the hardware. It functions as a standalone sampler utility without requiring a full DAW.

## ✨ Features

* ** instant Sampling:** Record audio directly from your system's output (PipeWire capture sink) onto any empty pad.
* **Visual Waveforms:** View waveform peaks, playhead position, and trim markers directly on the Push 2 high-res display.
* **Real-time Manipulation:**
    * **Volume:** Adjust individual sample gain.
    * **Pitch Shifting:** Real-time resampling for pitch adjustments (+/- 12 semitones).
    * **Trimming:** Non-destructive sample start and end point adjustments.
* **Smart Routing:** Toggle between local speakers, a virtual mixer sink, or both using hardware buttons.
* **State Persistence:** Samples are automatically saved to disk and remapped upon application restart.

## 🛠️ Tech Stack

* **Language:** Rust (2024 Edition)
* **Audio Backend:** [PipeWire](https://pipewire.org/) (`pipewire` crate)
* **Playback Engine:** [Kira](https://crates.io/crates/kira)
* **Hardware Interface:** `push2` (custom fork)
* **Graphics:** `embedded-graphics`
* **Async Runtime:** `tokio`

## ⚙️ Prerequisites

To run Pushboard, you need a Linux environment with PipeWire configured.

**System Requirements:**
* **Ableton Push 2** connected via USB.
* **Rust Toolchain** (latest stable).
* **PipeWire** server running.
* `pw-link` utility (usually part of `pipewire-utils` or `pipewire-jack`).

### Dependencies (CachyOS / Arch Linux)

```bash
sudo pacman -S pipewire pipewire-alsa pipewire-jack alsa-lib pkgconf libusb
```

### Dependencies (Ubuntu/Debian)

```bash
sudo apt install libasound2-dev libpipewire-0.3-dev pkg-config libusb-1.0-0-dev
```

## 🚀 Installation

1.  **Clone the repository:**
    ```bash
    git clone [https://github.com/yourusername/pushboard.git](https://github.com/yourusername/pushboard.git)
    cd pushboard
    ```

2.  **Build and Run:**
    Note: Ensure your Push 2 is powered on and connected before starting the application.
    ```bash
    cargo run --release
    ```

## 🎮 Usage Guide

Once the application is running, the Push 2 pads will light up.

### 🟥 Recording & Playback
* **Record:** Press any **Unlit** pad. The pad will turn **Red** to indicate recording is active. It captures the current system audio.
* **Stop Recording:** Press the flashing **Red** pad again. The sample is saved, and the pad turns **Blue**.
* **Play:** Press any **Blue** pad to trigger the sample. The pad turns **Pink** during playback.

### 🎛️ Editing Samples
Select a pad by pressing it (triggers playback) or by holding `Select` + Pad. The Push 2 display will show the waveform and the following parameters on the encoders:

| Encoder | Parameter | Description |
| :--- | :--- | :--- |
| **Track 1** | **Volume** | Adjust playback volume (-30dB to +15dB). |
| **Track 2** | **Pitch** | Pitch shift sample (+/- 12 Semitones). |
| **Track 3** | **Start** | Adjust sample start point. |
| **Track 4** | **End** | Adjust sample end point. |

### 🔘 Button Shortcuts
* **Delete + Pad:** Deletes the sample file and clears the pad.
* **Select + Pad:** Selects a pad for editing/viewing on the screen without triggering sound.
* **Mute / Solo:** Toggles audio routing targets (e.g., switch between local playback or routing to a virtual mixer sink via `pw-link`).

## 📂 Data Storage

Recordings are stored as 32-bit Float WAV files in your system's audio directory:

* **Linux:** `~/Music/soundboard-recordings/` (or equivalent XDG Audio dir)
* **Naming:** `pad_x_y.wav`

## 🔧 Configuration

The application uses `env_logger`. You can adjust logging verbosity using environment variables:

```bash
RUST_LOG=info cargo run --release
```

There is currently no external configuration file; audio routing targets (`alsa_playback.pushboard` to `MyMixer`) are defined as constants in `src/app/audio_player.rs`.

## 🤝 Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

## 📄 License

[MIT](https://choosealicense.com/licenses/mit/)

