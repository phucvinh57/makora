use std::sync::mpsc::Sender;

use anyhow::Result;

use crate::core::model::WatchEvent;

pub trait FocusWatcher {
    fn start(&mut self, tx: Sender<WatchEvent>) -> Result<()>;
}
