//! E2E Test Harness: Integrated test fixtures and simulation environment.
//!
//! Exposes:
//! - `MockKernel`: In-memory protocol broker for message exchanges.
//! - `GestureSimulator`: Virtual input driver (hotkeys, typing, clicks, holds).
//! - `A11yInspector`: AccessKit tree query and compliance auditor.
//! - `FrameInspector`: Software renderer frame and pixel buffer inspector.
//! - `TokenVerifier`: Design tokens TOML parser and multi-target exporter.
//! - `E2eHarness`: High-level integrated test runner fixture.

pub mod a11y_inspector;
pub mod frame_inspector;
pub mod gestures;
pub mod mock_kernel;
pub mod token_verifier;

pub use a11y_inspector::A11yInspector;
pub use frame_inspector::FrameInspector;
pub use gestures::{GestureSimulator, HoldOutcome};
pub use mock_kernel::{MockAuthorityPolicy, MockKernel};
pub use token_verifier::{TokenDoc, TokenVal, TokenVerifier};

use hakille_ui_desktop::app::{HudAction, HudApp};
use hakille_ui_desktop::runtime::FrameBudget;
use hakille_ui_desktop::scene::Scene;
use hakille_ui_model::StableSnapshot;
use hakille_ui_model::snapshot::{HUD_HEIGHT, HUD_WIDTH};
use hakille_ui_protocol::{ActionCard, KernelToUi};
use std::time::Duration;

/// High-level integrated E2E test harness.
pub struct E2eHarness {
    pub kernel: MockKernel,
    pub gestures: GestureSimulator,
    pub app: HudApp,
    pub frame_inspector: FrameInspector,
}

impl Default for E2eHarness {
    fn default() -> Self {
        Self::new_authorized()
    }
}

impl E2eHarness {
    /// Create harness with authorized kernel bridge.
    pub fn new_authorized() -> Self {
        let kernel = MockKernel::authorized();
        let app = HudApp::new(true, false, FrameBudget::battery_saver(), true);
        let frame_inspector = FrameInspector::new(HUD_WIDTH, HUD_HEIGHT, true);
        Self {
            kernel,
            gestures: GestureSimulator::new(),
            app,
            frame_inspector,
        }
    }

    /// Create harness with detached kernel bridge (fail-closed testing).
    pub fn new_detached() -> Self {
        let kernel = MockKernel::detached();
        let app = HudApp::new(false, false, FrameBudget::battery_saver(), true);
        let frame_inspector = FrameInspector::new(HUD_WIDTH, HUD_HEIGHT, true);
        Self {
            kernel,
            gestures: GestureSimulator::new(),
            app,
            frame_inspector,
        }
    }

    /// Advance virtual clock.
    pub fn advance_time(&mut self, duration: Duration) {
        self.gestures.advance_time(duration);
    }

    /// Press the global hotkey to toggle HUD visibility.
    pub fn press_hotkey(&mut self) -> Vec<HudAction> {
        self.gestures.press_hotkey(&mut self.app)
    }

    /// Type natural text into the HUD prompt.
    pub fn type_text(&mut self, text: &str) -> Vec<HudAction> {
        self.gestures.type_query(&mut self.app, text)
    }

    /// Submit HUD query with Enter key.
    pub fn submit_query(&mut self) -> Vec<HudAction> {
        self.gestures.press_enter(&mut self.app)
    }

    /// Inject Action Card from kernel into the HUD.
    pub fn inject_action_card(&mut self, card: ActionCard) {
        self.kernel.stage_card(card.clone());
        self.app.on_kernel_reply(KernelToUi::ShowCard(card));
    }

    /// Perform hold-to-confirm gesture on the pending action card.
    pub fn hold_validate(&mut self, duration: Duration) -> HoldOutcome {
        self.gestures.mouse_hold_validate(&mut self.app, duration)
    }

    /// Get current immutable snapshot.
    pub fn snapshot(&self) -> StableSnapshot {
        self.app.snapshot()
    }

    /// Render current scene.
    pub fn scene(&self) -> Scene {
        self.app.scene()
    }
}
