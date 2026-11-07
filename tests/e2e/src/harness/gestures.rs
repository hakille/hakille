//! GestureSimulator: Virtual user interaction simulator.
//!
//! Simulates hotkeys, keyboard typing, IME composition, mouse clicks,
//! window dragging hit-tests, and virtual time-based hold-to-confirm gestures.

use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};
use hakille_design::motion::HOLD_TO_CONFIRM_MS;
use hakille_ui_desktop::app::{HudAction, HudApp, HudInput, PointerTarget};
use std::time::Duration;

/// Result of a hold-to-confirm gesture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldOutcome {
    Confirmed { card_id: u64 },
    RejectedTooShort { elapsed_ms: u64, needed_ms: u64 },
    NoPendingCard,
}

/// Virtual input gesture simulator.
pub struct GestureSimulator {
    pub virtual_clock_ms: u64,
    pub hold_state: HoldState,
}

impl Default for GestureSimulator {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureSimulator {
    pub fn new() -> Self {
        Self {
            virtual_clock_ms: 0,
            hold_state: HoldState::Idle,
        }
    }

    /// Advance virtual clock without real sleep.
    pub fn advance_time(&mut self, duration: Duration) {
        self.virtual_clock_ms += duration.as_millis() as u64;
    }

    /// Press the global HUD hotkey (Ctrl+Alt+Space / Cmd+Space).
    pub fn press_hotkey(&mut self, app: &mut HudApp) -> Vec<HudAction> {
        app.handle(HudInput::HotkeyToggle)
    }

    /// Type a string character-by-character.
    pub fn type_query(&mut self, app: &mut HudApp, text: &str) -> Vec<HudAction> {
        let mut actions = Vec::new();
        for c in text.chars() {
            actions.extend(app.handle(HudInput::Char(c)));
        }
        actions
    }

    /// Commit IME text as a single atomic batch.
    pub fn commit_ime(&mut self, app: &mut HudApp, text: &str) -> Vec<HudAction> {
        app.handle(HudInput::ImeCommit(text.to_string()))
    }

    /// Press Enter key to submit HUD query.
    pub fn press_enter(&mut self, app: &mut HudApp) -> Vec<HudAction> {
        let actions = app.handle(HudInput::Enter);
        // If Enter produced a Submit action, automatically dispatch it to the bridge
        let mut follow_up = Vec::new();
        for action in &actions {
            if let HudAction::Submit { query } = action {
                follow_up.extend(app.submit_query(query.clone()));
            }
        }
        let mut all = actions;
        all.extend(follow_up);
        all
    }

    /// Press Escape key.
    pub fn press_escape(&mut self, app: &mut HudApp) -> Vec<HudAction> {
        app.handle(HudInput::Escape)
    }

    /// Press Backspace key.
    pub fn press_backspace(&mut self, app: &mut HudApp) -> Vec<HudAction> {
        app.handle(HudInput::Backspace)
    }

    /// Press F1 (Journal).
    pub fn press_f1(&mut self, app: &mut HudApp) -> Vec<HudAction> {
        app.handle(HudInput::ShowJournal)
    }

    /// Press F2 (Memory).
    pub fn press_f2(&mut self, app: &mut HudApp) -> Vec<HudAction> {
        app.handle(HudInput::ShowMemory)
    }

    /// Press F3 (Status).
    pub fn press_f3(&mut self, app: &mut HudApp) -> Vec<HudAction> {
        app.handle(HudInput::ShowStatus)
    }

    /// Press F4 (Agents).
    pub fn press_f4(&mut self, app: &mut HudApp) -> Vec<HudAction> {
        app.handle(HudInput::ShowAgents)
    }

    /// Send a Tick event to the app.
    pub fn tick(&mut self, app: &mut HudApp) -> Vec<HudAction> {
        app.handle(HudInput::Tick)
    }

    /// Perform a mouse click at logical coordinate (x, y).
    pub fn mouse_click(
        &mut self,
        app: &mut HudApp,
        x: f32,
        y: f32,
    ) -> (PointerTarget, Vec<HudAction>) {
        let target = app.pointer_target(x, y);
        let mut actions = Vec::new();
        match target {
            PointerTarget::CardValidate => {
                if let Some(card) = app.pending_card() {
                    let card_id = card.id;
                    if card.reversible {
                        // Reversible cards validate immediately without hold
                        actions.push(HudAction::ApproveCard { card_id });
                    } else {
                        // Irreversible card clicked without hold -> hold too short notice
                        actions.extend(app.on_card_hold_too_short());
                    }
                }
            }
            PointerTarget::CardEdit => {
                if let Some(card) = app.pending_card() {
                    actions.push(HudAction::EditCard { card_id: card.id });
                }
            }
            PointerTarget::CardReject => {
                if let Some(card) = app.pending_card() {
                    actions.push(HudAction::RejectCard { card_id: card.id });
                }
            }
            PointerTarget::Drag | PointerTarget::None => {}
        }
        (target, actions)
    }

    /// Simulate a virtual hold-to-confirm on the validation button for the specified duration.
    pub fn mouse_hold_validate(&mut self, app: &mut HudApp, duration: Duration) -> HoldOutcome {
        let Some(card) = app.pending_card() else {
            return HoldOutcome::NoPendingCard;
        };
        let card_id = card.id;
        let is_reversible = card.reversible;

        if is_reversible {
            return HoldOutcome::Confirmed { card_id };
        }

        // Start hold
        let start_ms = self.virtual_clock_ms;
        let (new_state, _) = feed_hold(HoldState::Idle, HoldEvent::Pressed { at_ms: start_ms });
        self.hold_state = new_state;

        // Advance virtual clock by the hold duration
        let hold_ms = duration.as_millis() as u64;
        self.virtual_clock_ms += hold_ms;

        let (final_state, _) = feed_hold(
            self.hold_state,
            HoldEvent::Tick {
                elapsed_ms: hold_ms,
            },
        );
        self.hold_state = final_state;

        if hold_ms >= HOLD_TO_CONFIRM_MS as u64 {
            let (confirmed_state, _) = feed_hold(
                self.hold_state,
                HoldEvent::Released {
                    at_ms: self.virtual_clock_ms,
                },
            );
            self.hold_state = confirmed_state;
            HoldOutcome::Confirmed { card_id }
        } else {
            let (cancelled_state, _) = feed_hold(
                self.hold_state,
                HoldEvent::Released {
                    at_ms: self.virtual_clock_ms,
                },
            );
            self.hold_state = cancelled_state;
            app.on_card_hold_too_short();
            HoldOutcome::RejectedTooShort {
                elapsed_ms: hold_ms,
                needed_ms: HOLD_TO_CONFIRM_MS as u64,
            }
        }
    }

    /// Query the pointer hit-testing target at (x, y).
    pub fn drag_window_check(&self, app: &HudApp, x: f32, y: f32) -> PointerTarget {
        app.pointer_target(x, y)
    }

    /// Get current hold progress (0.0 ..= 1.0).
    pub fn current_hold_progress(&self) -> f32 {
        hold_progress(self.hold_state)
    }
}
