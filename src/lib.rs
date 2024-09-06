use libflac_sys as ffi;
use soundkit::audio_packet::Encoder;
use std::cell::RefCell;
use std::os::raw::{c_char, c_int};
use std::rc::Rc;

const READSIZE: usize = 1024;

pub struct FlacEncoder {
    encoder: *mut ffi::FLAC__StreamEncoder,
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
    buffer: Rc<RefCell<Vec<u8>>>,
    frame_size: u32,
}

extern "C" fn write_callback(
    _encoder: *const ffi::FLAC__StreamEncoder,
    buffer: *const ffi::FLAC__byte,
    bytes: usize,
    _samples: u32,
    _current_frame: u32,
    client_data: *mut libc::c_void,
) -> ffi::FLAC__StreamEncoderWriteStatus {
    unsafe {
        let output = &mut *(client_data as *mut RefCell<Vec<u8>>);
        let slice = std::slice::from_raw_parts(buffer, bytes);
        output.borrow_mut().extend_from_slice(slice);
    }
    ffi::FLAC__STREAM_ENCODER_WRITE_STATUS_OK
}

impl Encoder for FlacEncoder {
    fn new(
        sample_rate: u32,
        bits_per_sample: u32,
        channels: u32,
        frame_size: u32,
        compression_level: u32,
    ) -> Self {
        let buffer = Rc::new(RefCell::new(Vec::new()));

        let encoder = unsafe {
            let encoder = ffi::FLAC__stream_encoder_new();
            ffi::FLAC__stream_encoder_set_verify(encoder, true as i32);
            ffi::FLAC__stream_encoder_set_compression_level(encoder, compression_level);
            ffi::FLAC__stream_encoder_set_channels(encoder, channels);
            ffi::FLAC__stream_encoder_set_bits_per_sample(encoder, bits_per_sample);
            ffi::FLAC__stream_encoder_set_sample_rate(encoder, sample_rate);
            ffi::FLAC__stream_encoder_set_total_samples_estimate(encoder, frame_size as u64);

            encoder
        };

        FlacEncoder {
            encoder,
            sample_rate,
            channels,
            bits_per_sample,
            buffer,
            frame_size,
        }
    }

    fn init(&mut self) -> Result<(), String> {
        let status = unsafe {
            ffi::FLAC__stream_encoder_init_stream(
                self.encoder,
                Some(write_callback),
                None, // seek callback
                None, // tell callback
                None, // metadata callback
                Rc::into_raw(self.buffer.clone()) as *mut libc::c_void,
            )
        };

        if status != ffi::FLAC__STREAM_ENCODER_INIT_STATUS_OK {
            return Err(format!(
                "Failed to initialize FLAC encoder, state: {}",
                status
            ));
        } else {
            Ok(())
        }
    }

    fn encode_i16(&mut self, input: &[i16], output: &mut [u8]) -> Result<usize, String> {
        Err("Not implemented.".to_string())
    }

    fn encode_i32(&mut self, input: &[i32], output: &mut [u8]) -> Result<usize, String> {
        self.buffer.borrow_mut().clear(); // Clear previous encoded data
        unsafe {
            let success = ffi::FLAC__stream_encoder_process_interleaved(
                self.encoder,
                input.as_ptr(),
                (input.len() / self.channels as usize) as u32,
            );

            if success == 0 {
                let state = ffi::FLAC__stream_encoder_get_state(self.encoder);
                return Err(format!(
                    "Failed to process samples, encoder state: {:?}",
                    state
                ));
            }
        }
        let encoded_data = self.buffer.borrow();
        let encoded_len = encoded_data.len();

        if output.len() < encoded_len {
            return Err("Output buffer too small".to_string());
        }

        output[..encoded_len].copy_from_slice(&encoded_data);
        Ok(encoded_len)
    }

    fn reset(&mut self) -> Result<(), String> {
        unsafe {
            ffi::FLAC__stream_encoder_finish(self.encoder);
            ffi::FLAC__stream_encoder_delete(self.encoder);

            self.encoder = ffi::FLAC__stream_encoder_new();
            ffi::FLAC__stream_encoder_set_verify(self.encoder, true as i32);
            ffi::FLAC__stream_encoder_set_compression_level(self.encoder, 5);
            ffi::FLAC__stream_encoder_set_channels(self.encoder, self.channels);
            ffi::FLAC__stream_encoder_set_bits_per_sample(self.encoder, self.bits_per_sample);
            ffi::FLAC__stream_encoder_set_sample_rate(self.encoder, self.sample_rate);

            let status = ffi::FLAC__stream_encoder_init_stream(
                self.encoder,
                Some(write_callback),
                None, // seek callback
                None, // tell callback
                None, // metadata callback
                Rc::into_raw(self.buffer.clone()) as *mut libc::c_void,
            );

            if status != ffi::FLAC__STREAM_ENCODER_INIT_STATUS_OK {
                let state: u32 = ffi::FLAC__stream_encoder_get_state(self.encoder);
                return Err(format!(
                    "Failed to reset encoder, encoder state: {:?}",
                    state
                ));
            }
        }

        Ok(())
    }
}

impl Drop for FlacEncoder {
    fn drop(&mut self) {
        unsafe {
            ffi::FLAC__stream_encoder_finish(self.encoder);
            ffi::FLAC__stream_encoder_delete(self.encoder);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;
    use std::fs::File;
    use std::io::Write;

    const SAMPLE_RATE: u32 = 44100;
    const CHANNELS: u32 = 2;
    const BITS_PER_SAMPLE: u32 = 16;
    const DURATION_SECS: u32 = 1;
    const FREQUENCY: f64 = 440.0; // A4 note

    fn generate_sine_wave(samples: &mut [i32]) {
        let total_samples = SAMPLE_RATE * DURATION_SECS;
        for i in 0..total_samples {
            let sample_value = ((i as f64 * FREQUENCY * 2.0 * PI / SAMPLE_RATE as f64).sin()
                * i16::MAX as f64) as i32;
            samples[i as usize * 2] = sample_value; // Left channel
            samples[i as usize * 2 + 1] = sample_value; // Right channel
        }
    }

    #[test]
    fn test_flac_encoder_with_sine_wave() {
        let total_samples = SAMPLE_RATE * DURATION_SECS;
        let mut samples = vec![0; (total_samples * CHANNELS) as usize];
        generate_sine_wave(&mut samples);

        let mut encoder =
            FlacEncoder::new(SAMPLE_RATE, BITS_PER_SAMPLE, CHANNELS, total_samples, 5);
        encoder.init().expect("Failed to initialize FLAC encoder");

        // Buffer to hold encoded data
        let mut encoded_data = vec![0u8; 1024 * 1024]; // 1MB buffer

        let encoded_len = encoder
            .encode_i32(&samples, &mut encoded_data)
            .expect("Failed to encode sine wave");

        // Write encoded data to a file
        let mut file =
            File::create("testdata/sinewave.flac").expect("Failed to create output file");
        file.write_all(&encoded_data[..encoded_len])
            .expect("Failed to write to output file");

        // Finish and clean up the encoder
        encoder.reset().expect("Failed to reset encoder");
    }
}
