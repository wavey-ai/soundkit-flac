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
    frame_length: u32,
    compression_level: u32,
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
        frame_length: u32,
        compression_level: u32,
    ) -> Self {
        let buffer = Rc::new(RefCell::new(Vec::new()));

        let encoder = unsafe {
            let encoder = ffi::FLAC__stream_encoder_new();
            encoder
        };

        FlacEncoder {
            encoder,
            sample_rate,
            channels,
            bits_per_sample,
            buffer,
            frame_length,
            compression_level,
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
            return Err(format!(
                "Output buffer of len {} too small for encoded data of len {}; input len was {}",
                output.len(),
                encoded_len,
                input.len(),
            ));
        }

        output[..encoded_len].copy_from_slice(&encoded_data);
        Ok(encoded_len)
    }

    fn reset(&mut self) -> Result<(), String> {
        unsafe {
            ffi::FLAC__stream_encoder_finish(self.encoder);
            ffi::FLAC__stream_encoder_delete(self.encoder);

            self.encoder = ffi::FLAC__stream_encoder_new();
            ffi::FLAC__stream_encoder_set_blocksize(self.encoder, self.frame_length);
            ffi::FLAC__stream_encoder_set_verify(self.encoder, true as i32);
            ffi::FLAC__stream_encoder_set_compression_level(self.encoder, self.compression_level);
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
    use soundkit::audio_bytes::{f32le_to_i32, s16le_to_i32, s24le_to_i32};
    use soundkit::wav::WavStreamProcessor;
    use std::fs::File;
    use std::io::Read;
    use std::io::Write;

    fn run_flac_encoder_with_wav_file(file_path: &str) {
        let frame_size = 4096;
        let mut file = File::open(file_path).unwrap();
        let mut file_buffer = Vec::new();
        file.read_to_end(&mut file_buffer).unwrap();

        let mut processor = WavStreamProcessor::new();
        let audio_data = processor.add(&file_buffer).unwrap().unwrap();

        let mut encoder = FlacEncoder::new(
            audio_data.sampling_rate(),
            audio_data.bits_per_sample() as u32,
            audio_data.channel_count() as u32,
            0 as u32,
            5,
        );
        encoder.init().expect("Failed to initialize FLAC encoder");

        let i32_samples = match audio_data.bits_per_sample() {
            16 => {
                // this doesn't scale the 16 bit samples - important!
                s16le_to_i32(audio_data.data())
            }
            24 => s24le_to_i32(audio_data.data()),
            32 => f32le_to_i32(audio_data.data()),
            _ => {
                vec![0i32]
            }
        };

        let mut encoded_data = Vec::new();
        let chunk_size = frame_size * audio_data.channel_count() as usize;

        for (i, chunk) in i32_samples.chunks(chunk_size).enumerate() {
            let mut output_buffer = vec![0u8; chunk.len() * std::mem::size_of::<i32>() * 10];

            match encoder.encode_i32(chunk, &mut output_buffer) {
                Ok(encoded_len) => {
                    println!(
                        "Chunk {}: Input size = {} bytes, Encoded size = {} bytes",
                        i,
                        chunk.len() * std::mem::size_of::<i32>(),
                        encoded_len
                    );
                    encoded_data.extend_from_slice(&output_buffer[..encoded_len]);
                }
                Err(e) => {
                    panic!("Failed to encode chunk {}: {:?}", i, e);
                }
            }
        }

        let mut file =
            File::create(file_path.to_owned() + ".flac").expect("Failed to create output file");
        file.write_all(&encoded_data)
            .expect("Failed to write to output file");

        encoder.reset().expect("Failed to reset encoder");
    }

    #[test]
    fn test_flac_encoder_with_wave_16bit() {
        run_flac_encoder_with_wav_file("testdata/s16le.wav");
    }

    #[test]
    fn test_flac_encoder_with_wave_24bit() {
        run_flac_encoder_with_wav_file("testdata/s24le.wav");
    }

    #[test]
    fn test_flac_encoder_with_wave_32bit() {
        run_flac_encoder_with_wav_file("testdata/f32le.wav");
    }

    #[test]
    fn test_flac_encoder_with_wave_s32bit() {
        run_flac_encoder_with_wav_file("testdata/s32le.wav");
    }
}
