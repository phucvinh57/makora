//! The core's events, carried from its worker threads to the UI thread.

use async_channel::{Receiver, Sender};
use makora_lib::events::{CoreEvent, EventSink};

pub struct ChannelEvents {
    tx: Sender<CoreEvent>,
}

impl ChannelEvents {
    pub fn new() -> (Self, Receiver<CoreEvent>) {
        let (tx, rx) = async_channel::unbounded();
        (Self { tx }, rx)
    }
}

impl EventSink for ChannelEvents {
    fn emit(&self, event: CoreEvent) {
        let _ = self.tx.try_send(event);
    }
}
