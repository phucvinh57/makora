//! Native-messaging host the browser extension talks to. The browser starts it per user and
//! pipes the extension's messages over stdin; it relays each one to the running app and drops
//! it when the app is down.

#[path = "../tracking/adapters/bridge_wire.rs"]
mod bridge_wire;

use std::io::{self, Write};
use std::process::ExitCode;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::Stream;

use bridge_wire::{read_frame, socket_name, Frame, MAX_FRAME_BYTES};

fn main() -> ExitCode {
    let mut stdin = io::stdin().lock();
    let mut app = None;
    loop {
        match read_frame(&mut stdin) {
            Ok(Some(Frame::Message(payload))) => relay(&mut app, &payload),
            Ok(Some(Frame::Oversized)) => {}
            Ok(None) => return ExitCode::SUCCESS,
            Err(_) => return ExitCode::FAILURE,
        }
    }
}

/// One reconnect per message: the app may have restarted since the last one.
fn relay(app: &mut Option<Stream>, payload: &[u8]) {
    for _ in 0..2 {
        if app.is_none() {
            *app = socket_name().and_then(Stream::connect).ok();
        }
        let Some(stream) = app.as_mut() else {
            return;
        };
        if write_frame(stream, payload).is_ok() {
            return;
        }
        *app = None;
    }
}

fn write_frame(writer: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .ok()
        .filter(|len| *len <= MAX_FRAME_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    let mut frame = Vec::with_capacity(payload.len() + 4);
    frame.extend_from_slice(&len.to_ne_bytes());
    frame.extend_from_slice(payload);
    writer.write_all(&frame)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_frames_read_back() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, br#"{"browser":"chrome"}"#).unwrap();

        assert_eq!(
            read_frame(&mut buffer.as_slice()).unwrap(),
            Some(Frame::Message(br#"{"browser":"chrome"}"#.to_vec()))
        );
    }

    #[test]
    fn refuses_to_write_an_oversized_frame() {
        let err = write_frame(&mut Vec::new(), &vec![b'x'; MAX_FRAME_BYTES as usize + 1]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
