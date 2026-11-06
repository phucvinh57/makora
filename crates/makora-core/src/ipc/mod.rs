//! Messages between Makora's processes over a child's stdin and stdout: JSON in the bridge's
//! length-prefixed frames.

pub mod window;
pub mod worker;

use std::io::{self, Read, Write};

use anyhow::anyhow;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::categorization::application::ai_run::AiRunError;
use crate::tracking::adapters::bridge_wire::{read_frame, Frame, MAX_FRAME_BYTES};

/// [`AiRunError`] as it crosses between processes.
#[derive(Debug, Serialize, Deserialize)]
pub enum RunFailure {
    AlreadyRunning,
    NoCategories,
    Failed(String),
}

impl From<AiRunError> for RunFailure {
    fn from(err: AiRunError) -> Self {
        match err {
            AiRunError::AlreadyRunning => Self::AlreadyRunning,
            AiRunError::NoCategories => Self::NoCategories,
            AiRunError::Failed(err) => Self::Failed(format!("{err:#}")),
        }
    }
}

impl From<RunFailure> for AiRunError {
    fn from(failure: RunFailure) -> Self {
        match failure {
            RunFailure::AlreadyRunning => Self::AlreadyRunning,
            RunFailure::NoCategories => Self::NoCategories,
            RunFailure::Failed(message) => Self::Failed(anyhow!(message)),
        }
    }
}

pub fn write_message(writer: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let payload = serde_json::to_vec(message)?;
    let len = u32::try_from(payload.len())
        .ok()
        .filter(|len| *len <= MAX_FRAME_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "message too large"))?;
    writer.write_all(&len.to_ne_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

/// The next message, or `None` at a clean end of stream.
pub fn read_message<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<Option<T>> {
    match read_frame(reader)? {
        None => Ok(None),
        Some(Frame::Message(payload)) => Ok(Some(serde_json::from_slice(&payload)?)),
        Some(Frame::Oversized) => Err(io::Error::new(io::ErrorKind::InvalidData, "message too large")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::CoreEvent;

    #[test]
    fn messages_round_trip_then_end() {
        let mut bytes = Vec::new();
        write_message(&mut bytes, &CoreEvent::CategorizeProgress { done: 1, total: 2 }).unwrap();
        write_message(&mut bytes, &CoreEvent::CategorizeFinished).unwrap();
        let mut reader = bytes.as_slice();
        assert_eq!(
            read_message::<CoreEvent>(&mut reader).unwrap(),
            Some(CoreEvent::CategorizeProgress { done: 1, total: 2 })
        );
        assert_eq!(read_message::<CoreEvent>(&mut reader).unwrap(), Some(CoreEvent::CategorizeFinished));
        assert_eq!(read_message::<CoreEvent>(&mut reader).unwrap(), None);
    }
}
