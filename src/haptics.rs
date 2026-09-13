//! Tactile feedback for UI interactions (sliders, toggles, clicks).
//!
//! On macOS, triggers Force Touch / Taptic Engine feedback using `NSHapticFeedbackManager`.
//! On Linux and Windows, acts as a safe no-op.

use std::sync::atomic::{AtomicBool, Ordering};

static HAPTICS_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_haptics_enabled(enabled: bool) {
    HAPTICS_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn haptics_enabled() -> bool {
    HAPTICS_ENABLED.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HapticPattern {
    /// Standard click / tap (buttons, menu items, tabs).
    Generic,
    /// Alignment / notch detent (toggles, snapping to min/max).
    Alignment,
    /// Level change (scrubbing sliders).
    LevelChange,
}

#[cfg(target_os = "macos")]
pub fn trigger(pattern: HapticPattern) {
    if !haptics_enabled() {
        return;
    }
    use objc2_app_kit::{
        NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime,
        NSHapticFeedbackPerformer,
    };

    let pattern = match pattern {
        HapticPattern::Generic => NSHapticFeedbackPattern::Generic,
        HapticPattern::Alignment => NSHapticFeedbackPattern::Alignment,
        HapticPattern::LevelChange => NSHapticFeedbackPattern::LevelChange,
    };
    NSHapticFeedbackManager::defaultPerformer()
        .performFeedbackPattern_performanceTime(pattern, NSHapticFeedbackPerformanceTime::Now);
}

#[cfg(not(target_os = "macos"))]
pub fn trigger(_pattern: HapticPattern) {}
