//! What the core tells the front end while it works in the background. The front end supplies the
//! [`EventSink`] and decides how each event is shown.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTarget {
    CategoriesReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreEvent {
    CategorizeStarted { recheck: bool },
    CategorizeProgress { done: usize, total: usize },
    CategorizeFinished,
    DownloadProgress { downloaded: u64, total: u64 },
    DownloadFinished,
    Navigate(NavTarget),
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: CoreEvent);
}
