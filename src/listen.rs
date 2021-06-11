use crate::Res;
use std::collections::VecDeque;
use std::sync::mpsc;
use wasapi::*;

// Playback loop, play samples received from channel
pub fn playback_loop(
    rx_play: std::sync::mpsc::Receiver<Vec<u8>>,
    bits: usize,
    rate: usize,
) -> Res<()> {
    let device = get_default_device(&Direction::Render)?;
    let mut audio_client = device.get_iaudioclient()?;
    let desired_format = WaveFormat::new(bits, bits, &SampleType::Int, rate, 2);

    let blockalign = desired_format.get_blockalign();
    debug!("Desired playback format: {:?}", desired_format);

    let (def_time, min_time) = audio_client.get_periods()?;
    debug!("default period {}, min period {}", def_time, min_time);

    audio_client.initialize_client(
        &desired_format,
        min_time as i64,
        &Direction::Render,
        &ShareMode::Shared,
        true,
    )?;
    debug!("initialized playback");

    let h_event = audio_client.set_get_eventhandle()?;

    let mut buffer_frame_count = audio_client.get_bufferframecount()?;

    let render_client = audio_client.get_audiorenderclient()?;
    let mut sample_queue: VecDeque<u8> = VecDeque::with_capacity(
        100 * blockalign as usize * (1024 + 2 * buffer_frame_count as usize),
    );
    audio_client.start_stream()?;
    loop {
        buffer_frame_count = audio_client.get_available_space_in_frames()?;
        trace!("New buffer frame count {}", buffer_frame_count);
        while sample_queue.len() < (blockalign as usize * buffer_frame_count as usize) {
            debug!("need more samples");
            match rx_play.try_recv() {
                Ok(chunk) => {
                    trace!("got chunk");
                    for element in chunk.iter() {
                        sample_queue.push_back(*element);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    warn!("no data, filling with zeros");
                    for _ in 0..((blockalign as usize * buffer_frame_count as usize)
                        - sample_queue.len())
                    {
                        sample_queue.push_back(0);
                    }
                }
                Err(_) => {
                    error!("Channel is closed");
                    break;
                }
            }
        }

        trace!("write");
        render_client.write_to_device_from_deque(
            buffer_frame_count as usize,
            blockalign as usize,
            &mut sample_queue,
        )?;
        trace!("write ok");
        if h_event.wait_for_event(100000).is_err() {
            error!("error, stopping playback");
            audio_client.stop_stream()?;
            break;
        }
    }
    Ok(())
}
