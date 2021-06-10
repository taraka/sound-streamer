
use clap::{App, Arg};
use std::sync::mpsc;
use std::thread;
use windows::initialize_mta;
use stream::capture_loop;
use listen::playback_loop;
use std::error;
use std::net::UdpSocket;
use std::convert::TryInto;

#[macro_use]
extern crate log;
use simplelog::*;

mod listen;
mod stream;


pub type Res<T> = Result<T, Box<dyn error::Error>>;

// Main loop
fn main() -> Res<()> {

    let matches = App::new("srtpsrv")
    .arg(
        Arg::with_name("LISTEN")
            .short("l")
            .long("listen")
            .help("Listen to some tunes")
            .takes_value(false)
    ).arg(
        Arg::with_name("STREAM")
            .short("s")
            .long("stream")
            .help("Send your audio to a friend")
            .takes_value(false)
    ).arg(
        Arg::with_name("ADDR")
            .short("i")
            .long("ip")
            .help("ip address")
            .takes_value(true)
            .default_value("127.0.0.1")
    ).arg(
        Arg::with_name("PORT")
            .short("p")
            .long("port")
            .help("UDP port to use")
            .takes_value(true)
            .default_value("6969")
    ).get_matches();


    let is_listen_mode = matches.is_present("LISTEN");
    let is_stream_mode = matches.is_present("STREAM");
    let port = matches.value_of("PORT").unwrap().parse::<u16>().unwrap();
    let addr = matches.value_of("ADDR").unwrap().to_string();

    let _ = SimpleLogger::init(
        LevelFilter::Debug,
        ConfigBuilder::new()
            .set_time_format_str("%H:%M:%S%.3f")
            .build(),
    );

    initialize_mta().unwrap();

    match (is_listen_mode, is_stream_mode) {
        (true, false) => start_listening(port),
        (false, true) => start_streaming(addr, port),
        (true, true) => error!("You can't listen and stream from the same app"),
        (false, false) => error!("you've got to choose what I'm meant to be doing (maybe look at --help)"),
    };

    Ok(())
}


fn start_listening(port: u16) {
    let (tx_play, rx_play): (
        std::sync::mpsc::SyncSender<Vec<u8>>,
        std::sync::mpsc::Receiver<Vec<u8>>,
    ) = mpsc::sync_channel(2);
    let (tx_capt, rx_capt): (
        std::sync::mpsc::SyncSender<Vec<u8>>,
        std::sync::mpsc::Receiver<Vec<u8>>,
    ) = mpsc::sync_channel(2);

    // Playback
    let _handle = thread::Builder::new()
        .name("Player".to_string())
        .spawn(move || {
            let result = playback_loop(rx_play);
            if let Err(err) = result {
                error!("Playback failed with error: {}", err);
            }
        });

    // Capture
    let _handle = thread::Builder::new()
        .name("Network".to_string())
        .spawn(move || {
            let result = udp_recv_loop(tx_capt, port);
            if let Err(err) = result {
                error!("Recv error: {}", err);
            }
        });

    loop {
        match rx_capt.recv() {
            Ok(chunk) => {
                
                debug!("sending");
                tx_play.send(chunk).unwrap();
            }
            Err(err) => error!("Some error {}", err),
        }
    }
}


fn start_streaming(addr: String, port: u16) {
    let (tx_play, rx_play): (
        std::sync::mpsc::SyncSender<Vec<u8>>,
        std::sync::mpsc::Receiver<Vec<u8>>,
    ) = mpsc::sync_channel(2);
    let (tx_capt, rx_capt): (
        std::sync::mpsc::SyncSender<Vec<u8>>,
        std::sync::mpsc::Receiver<Vec<u8>>,
    ) = mpsc::sync_channel(2);
    let chunksize = 4096;

    // Playback
    let _handle = thread::Builder::new()
        .name("Network".to_string())
        .spawn(move || {
            let result = udp_send_loop(rx_play, &addr, port);
            if let Err(err) = result {
                error!("send failed with error {}", err);
            }
        });

    // Capture
    let _handle = thread::Builder::new()
        .name("Capture".to_string())
        .spawn(move || {
            let result = capture_loop(tx_capt, chunksize);
            if let Err(err) = result {
                error!("Capture failed with error {}", err);
            }
        });

    loop {
        match rx_capt.recv() {
            Ok(chunk) => {
                tx_play.send(chunk).unwrap();
            }
            Err(err) => error!("Some error {}", err),
        }
    }
}

fn udp_send_loop(rx_play: std::sync::mpsc::Receiver<Vec<u8>>, addr: &str, port: u16) -> Res<()>  {

    let host = format!("{}:{}", addr, port);

    let mut socket = UdpSocket::bind("0.0.0.0:3400").expect("couldn't bind to address");
    socket.connect(&host).expect("connect function failed");

    loop {
        match rx_play.recv() {
            Ok(chunk) => {
                debug!("sending {}", chunk.len());
                socket.send(&chunk).expect("couldn't send message");
            }
            Err(err) => error!("Some error {}", err),
        }
    }

    Ok(())
}


fn udp_recv_loop(tx_capt: std::sync::mpsc::SyncSender<Vec<u8>>, port: u16) -> Res<()>  {

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))?;

    // Receives a single datagram message on the socket. If `buf` is too small to hold
    // the message, it will be cut off.
    let mut buf = [0; 32768];

    loop {
        let (amt, src) = socket.recv_from(&mut buf)?;
        error!("Got {} bytes from {}", amt, src);
        tx_capt.send(buf.to_vec());
    }

    Ok(())
}