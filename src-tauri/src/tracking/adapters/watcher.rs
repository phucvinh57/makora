use std::sync::mpsc::Sender;

use anyhow::Result;

use super::model::WatchEvent;

pub trait FocusWatcher {
    fn start(&mut self, tx: Sender<WatchEvent>) -> Result<()>;
}
