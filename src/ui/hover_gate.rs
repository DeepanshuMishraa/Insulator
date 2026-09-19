//! Calm hover highlights in fast-scrolling virtualized lists.
//!
//! GPUI re-evaluates `hover()` styles every frame from the current mouse
//! position, so when rows slide under a stationary cursor during a fast
//! scroll the highlight strobes across rows trying to keep up — and every
//! change costs a full-window repaint. The gate hides row hover backgrounds
//! while its list is moving and repaints once, after the list has been
//! still, so the highlight calmly reappears under the cursor.
//!
//! Usage: each scrolling list owns one gate. The list's scroll handler
//! calls [`HoverGate::note_scrolled`]; row builders consult
//! [`HoverGate::hover_allowed`] inside their `hover()` closures.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{App, Window};

/// Stillness required before hover highlights may paint again.
const RESTORE_AFTER: Duration = Duration::from_millis(140);

/// Tracks scroll activity for one list. Cheap to clone; shares state.
#[derive(Clone, Debug, Default)]
pub struct HoverGate {
    last_scroll: Rc<Cell<Option<Instant>>>,
    wake_armed: Rc<Cell<bool>>,
}

impl HoverGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when hover highlights may paint: the list never scrolled, or it
    /// has been still for [`RESTORE_AFTER`]. Pure, so the timing is testable.
    pub fn hover_allowed(&self) -> bool {
        match self.last_scroll.get() {
            None => true,
            Some(last) => last.elapsed() >= RESTORE_AFTER,
        }
    }

    /// Record a scroll event. Arms a one-shot wake that repaints once the
    /// list has been still long enough for hover to return; while scrolling
    /// continues the single in-flight wake just keeps waiting, so a long
    /// gesture costs no extra frames.
    pub fn note_scrolled(&self, window: &mut Window, cx: &mut App) {
        self.last_scroll.set(Some(Instant::now()));
        if self.wake_armed.replace(true) {
            return;
        }
        let state = self.clone();
        let view = window.current_view();
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(RESTORE_AFTER + Duration::from_millis(32))
                    .await;
                if state.hover_allowed() {
                    state.wake_armed.set(false);
                    cx.update(|cx| cx.notify(view));
                    break;
                }
            }
        })
        .detach();
    }

    #[cfg(test)]
    fn set_last_scroll(&self, last: Option<Instant>) {
        self.last_scroll.set(last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_paints_before_any_scroll() {
        assert!(HoverGate::new().hover_allowed());
    }

    #[test]
    fn hover_hides_right_after_a_scroll_and_returns_once_still() {
        let gate = HoverGate::new();
        gate.set_last_scroll(Some(Instant::now()));
        assert!(!gate.hover_allowed());

        gate.set_last_scroll(Some(
            Instant::now()
                .checked_sub(RESTORE_AFTER + Duration::from_millis(50))
                .unwrap(),
        ));
        assert!(gate.hover_allowed());
    }
}
