//! What category an activity is filed under, and why.
//!
//! An activity can carry a **pin** (the user's own decision) and an **AI suggestion** (the
//! model's, possibly since rejected by the user). The rules that tie them together live here and
//! nowhere else:
//!
//! - a pin always wins over a suggestion when the category is resolved, and a user-written rule
//!   (see [`super::rules`]) ranks between the two: it yields to a pin and beats a suggestion;
//! - unpinning rejects whatever suggestion is active, so "Uncategorized" sticks and the model
//!   learns from it;
//! - the model can never overwrite a pin — a suggestion made for a pinned activity is dropped;
//! - accepting a suggestion turns it into a pin (the suggestion stays as the record of what the
//!   model said, which is how a confirmation is told from a plain pin);
//! - merging one activity into another carries a pin along unless the two disagree.

use serde::{Deserialize, Serialize};

use crate::shared::ids::{ActivityId, CategoryId};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AiSuggestion {
    pub category: CategoryId,
    pub rejected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinConflict;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityClassification {
    activity_id: ActivityId,
    pin: Option<CategoryId>,
    suggestion: Option<AiSuggestion>,
}

impl ActivityClassification {
    pub fn new(activity_id: ActivityId, pin: Option<CategoryId>, suggestion: Option<AiSuggestion>) -> Self {
        Self {
            activity_id,
            pin,
            suggestion,
        }
    }

    pub fn activity_id(&self) -> ActivityId {
        self.activity_id
    }

    pub fn pin(&self) -> Option<CategoryId> {
        self.pin
    }

    pub fn suggestion(&self) -> Option<&AiSuggestion> {
        self.suggestion.as_ref()
    }

    pub fn active_suggestion(&self) -> Option<CategoryId> {
        self.suggestion
            .filter(|suggestion| !suggestion.rejected)
            .map(|suggestion| suggestion.category)
    }

    pub fn is_pinned(&self) -> bool {
        self.pin.is_some()
    }

    pub fn pin_to(&mut self, category: CategoryId) {
        self.pin = Some(category);
    }

    pub fn unpin(&mut self) {
        self.pin = None;
        if let Some(suggestion) = &mut self.suggestion {
            suggestion.rejected = true;
        }
    }

    /// The model proposes `category`, replacing whatever it said before — a rejected suggestion
    /// included. A pinned activity is not the model's to change, so it is ignored.
    pub fn suggest(&mut self, category: CategoryId) {
        if self.is_pinned() {
            return;
        }
        self.suggestion = Some(AiSuggestion {
            category,
            rejected: false,
        });
    }

    /// The model no longer stands by its suggestion. A rejection is the user's, and stays.
    pub fn withdraw_suggestion(&mut self) {
        if self.is_pinned() {
            return;
        }
        if self.active_suggestion().is_some() {
            self.suggestion = None;
        }
    }

    /// Puts back the pin and rejection `prior` had, undoing a [`Self::pin_to`] or
    /// [`Self::unpin`]. A suggestion the model has replaced since is left as it is.
    pub fn undo_assignment(&mut self, prior: &Self) {
        self.pin = prior.pin;
        if let (Some(current), Some(prior)) = (&mut self.suggestion, prior.suggestion) {
            if current.category == prior.category {
                current.rejected = prior.rejected;
            }
        }
    }

    pub fn accept_suggestion(&mut self) {
        if self.is_pinned() {
            return;
        }
        self.pin = self.active_suggestion();
    }

    /// Carries this activity's pin onto `target`, which it is being merged into. A target that
    /// already has its own pin keeps it; two different pins refuse the merge.
    pub fn carry_pin_into(&self, target: &mut Self) -> Result<(), PinConflict> {
        match (self.pin, target.pin) {
            (Some(mine), Some(theirs)) if mine != theirs => Err(PinConflict),
            (Some(mine), None) => {
                target.pin = Some(mine);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
