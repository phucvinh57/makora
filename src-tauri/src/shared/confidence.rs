use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Confidence(f64);

impl Confidence {
    /// Clamps into range; a `NaN` becomes 0 rather than poisoning a comparison later.
    pub fn new(value: f64) -> Self {
        if value.is_nan() {
            Self(0.0)
        } else {
            Self(value.clamp(0.0, 1.0))
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}
