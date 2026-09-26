//! Skate 3 flick-it input: stick paths recognised as tricks.
//!
//! Ported from skate-3-rust-engine (`crates/skate-core/src/input/gesture.rs`
//! and `crates/skate-data/src/gesture_patterns.rs`), a reimplementation of
//! Skate 3's own recogniser. The gesture data itself (`skater.pat` and
//! friends) comes from the player's Skate 3 disc and is never shipped here.

mod gesture;
mod pat;

pub use gesture::{Pattern, Recognition, Recognizer, Settings};
pub use pat::{load_pat, parse_pat};

