use hound::{WavReader, WavSpec, WavWriter};
use std::env;
use std::io;
use std::path::{Path, PathBuf};

/// Creates a temporary, processed (pitched and trimmed) copy of a WAV file.
///
/// This is a synchronous function and should be called from a
/// non-blocking context (e.g., `tokio::task::spawn_blocking`).
///
/// It works by reading all samples, slicing them based on start/end points,
/// and then writing the slice to a new file with a modified (pitched) sample rate.
// ‼️ RENAMED function and ADDED start/end point parameters
pub fn create_processed_copy_sync(
    original_path: &Path,
    semitone_shift: f64,
    start_point: f64,
    end_point: f64,
) -> io::Result<PathBuf> {
    // 1. Calculate the pitch ratio (e.g., ~0.943 for -1 semitone)
    let pitch_ratio = 2.0_f64.powf(semitone_shift / 12.0);

    // 2. Open the original file
    let mut reader = WavReader::open(original_path).map_err(io::Error::other)?;
    let in_spec = reader.spec();

    // ‼️ ADDED: Calculate sample indices from normalized 0.0-1.0 points
    // `reader.len()` is total samples (e.g., 1000 frames * 2 channels = 2000)
    // `in_spec.channels` is the number of channels
    let total_samples = reader.len() as f64;
    let num_channels = in_spec.channels as f64;

    // `total_frames` is the number of "time slices" (e.g., 1000 for a stereo file)
    let total_frames = total_samples / num_channels;

    // Calculate start/end frames based on total frames
    let start_frame = (total_frames * start_point.clamp(0.0, 1.0)).round() as u32;
    let end_frame = (total_frames * end_point.clamp(0.0, 1.0)).round() as u32;

    // Convert frame indices to sample indices (which is what we iterate over)
    let start_sample_idx = (start_frame * in_spec.channels as u32) as usize;
    let end_sample_idx = (end_frame * in_spec.channels as u32) as usize;
    // ‼️ End of added section

    // 3. Calculate the new spec with the modified sample rate
    let new_sample_rate = (in_spec.sample_rate as f64 * pitch_ratio).round() as u32;
    let out_spec = WavSpec {
        channels: in_spec.channels,
        sample_rate: new_sample_rate,
        bits_per_sample: in_spec.bits_per_sample,
        sample_format: in_spec.sample_format,
    };

    // 4. Create a unique path for the temporary file
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_micros();
    // ‼️ RENAMED temp file
    let temp_file_path = env::temp_dir().join(format!("processed_sample_{}.wav", unique_id));

    // 5. Create the writer for the new temp file
    let mut writer = WavWriter::create(&temp_file_path, out_spec).map_err(io::Error::other)?;

    // 6. Copy samples, handling the different possible WAV formats
    //    We must match the format we are reading.
    match (in_spec.sample_format, in_spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => {
            // ‼️ Collect all samples into memory
            let samples: Vec<i16> = reader
                .samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(io::Error::other)?;
            // ‼️ Get the specified slice, or an empty slice if out of bounds
            let trimmed_samples = samples.get(start_sample_idx..end_sample_idx).unwrap_or(&[]);
            // ‼️ Write only the trimmed samples
            for &sample in trimmed_samples {
                writer.write_sample(sample).map_err(io::Error::other)?;
            }
        }
        (hound::SampleFormat::Int, 32) => {
            // ‼️ Collect all samples into memory
            let samples: Vec<i32> = reader
                .samples::<i32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(io::Error::other)?;
            // ‼️ Get the specified slice, or an empty slice if out of bounds
            let trimmed_samples = samples.get(start_sample_idx..end_sample_idx).unwrap_or(&[]);
            // ‼️ Write only the trimmed samples
            for &sample in trimmed_samples {
                writer.write_sample(sample).map_err(io::Error::other)?;
            }
        }
        (hound::SampleFormat::Float, 32) => {
            // This is the format our pipewire_source creates
            // ‼️ Collect all samples into memory
            let samples: Vec<f32> = reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(io::Error::other)?;
            // ‼️ Get the specified slice, or an empty slice if out of bounds
            let trimmed_samples = samples.get(start_sample_idx..end_sample_idx).unwrap_or(&[]);
            // ‼️ Write only the trimmed samples
            for &sample in trimmed_samples {
                writer.write_sample(sample).map_err(io::Error::other)?;
            }
        }
        (hound::SampleFormat::Int, 24) => {
            // hound reads 24-bit samples as i32
            // ‼️ Collect all samples into memory
            let samples: Vec<i32> = reader
                .samples::<i32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(io::Error::other)?;
            // ‼️ Get the specified slice, or an empty slice if out of bounds
            let trimmed_samples = samples.get(start_sample_idx..end_sample_idx).unwrap_or(&[]);
            // ‼️ Write only the trimmed samples
            for &sample in trimmed_samples {
                writer.write_sample(sample).map_err(io::Error::other)?;
            }
        }
        _ => {
            // If we encounter an unsupported format, return an error.
            return Err(io::Error::other(format!(
                "Unsupported WAV format: {:?}, {}-bit",
                in_spec.sample_format, in_spec.bits_per_sample
            )));
        }
    }
    // 7. Finalize the file and return the path
    writer.finalize().map_err(io::Error::other)?;
    Ok(temp_file_path)
}

