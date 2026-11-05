use std::sync::mpsc::Sender;

use chrono::Utc;

use super::model::{PresenceEvent, WatchEvent};

/// Whether someone is at the machine, from the signals a platform reports: nobody is while the
/// session is locked, the machine sleeps or its screens are off. Publishes only when that answer
/// changes. Every setter returns whether the tracker is still listening.
#[derive(Default)]
pub struct PresenceState {
    locked: bool,
    sleeping: bool,
    screens_off: bool,
    last_present: Option<bool>,
}

impl PresenceState {
    pub fn apply_snapshot(&mut self, locked: bool, sleeping: bool, tx: &Sender<WatchEvent>) -> bool {
        self.locked = locked;
        self.sleeping = sleeping;
        self.publish_if_changed(tx)
    }

    pub fn set_locked(&mut self, locked: bool, tx: &Sender<WatchEvent>) -> bool {
        self.locked = locked;
        self.publish_if_changed(tx)
    }

    pub fn set_sleeping(&mut self, sleeping: bool, tx: &Sender<WatchEvent>) -> bool {
        self.sleeping = sleeping;
        self.publish_if_changed(tx)
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn set_screens_off(&mut self, screens_off: bool, tx: &Sender<WatchEvent>) -> bool {
        self.screens_off = screens_off;
        self.publish_if_changed(tx)
    }

    fn transition(&mut self) -> Option<bool> {
        let present = !self.locked && !self.sleeping && !self.screens_off;
        if self.last_present == Some(present) {
            return None;
        }
        self.last_present = Some(present);
        Some(present)
    }

    fn publish_if_changed(&mut self, tx: &Sender<WatchEvent>) -> bool {
        let Some(present) = self.transition() else {
            return true;
        };
        tx.send(
            PresenceEvent {
                present,
                timestamp: Utc::now(),
            }
            .into(),
        )
        .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn published(rx: &mpsc::Receiver<WatchEvent>) -> Vec<bool> {
        rx.try_iter()
            .map(|event| match event {
                WatchEvent::Presence(event) => event.present,
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    #[test]
    fn publishes_only_when_presence_flips() {
        let (tx, rx) = mpsc::channel();
        let mut state = PresenceState::default();
        state.apply_snapshot(false, false, &tx);
        state.set_locked(true, &tx);
        state.set_sleeping(true, &tx);
        state.set_locked(false, &tx);
        state.set_sleeping(false, &tx);
        state.set_sleeping(false, &tx);
        assert_eq!(published(&rx), [true, false, true]);
    }

    #[test]
    fn screens_off_counts_as_away() {
        let (tx, rx) = mpsc::channel();
        let mut state = PresenceState::default();
        state.set_screens_off(true, &tx);
        state.set_screens_off(false, &tx);
        assert_eq!(published(&rx), [false, true]);
    }

    #[test]
    fn reports_a_closed_channel() {
        let (tx, rx) = mpsc::channel();
        drop(rx);
        assert!(!PresenceState::default().set_locked(true, &tx));
    }
}
