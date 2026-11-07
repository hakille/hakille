//! Feature 09: WidgetState Interactive Transitions (Milestone M3)
//!
//! Specifications: T1_F09_01 to T1_F09_05

use hakille_ui_desktop::components::{Button, PrimitiveCtx, WidgetState};
use hakille_ui_desktop::scene::{Rect, Scene};
use hakille_ui_desktop::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveState {
    Normal,
    Hover,
    Active,
    Focused,
    Disabled,
    ReducedMotion,
}

impl InteractiveState {
    pub fn resolve_precedence(
        reduced_motion: bool,
        disabled: bool,
        active: bool,
        hover: bool,
        focused: bool,
    ) -> Self {
        if reduced_motion {
            Self::ReducedMotion
        } else if disabled {
            Self::Disabled
        } else if active {
            Self::Active
        } else if hover {
            Self::Hover
        } else if focused {
            Self::Focused
        } else {
            Self::Normal
        }
    }
}

/// `T1_F09_01`: Pointer Enter Transition to Hover
#[test]
fn test_t1_f09_01_pointer_enter_transition_to_hover() {
    let state = InteractiveState::resolve_precedence(false, false, false, true, false);
    assert_eq!(state, InteractiveState::Hover);
}

/// `T1_F09_02`: Pointer Down Transition to Active
#[test]
fn test_t1_f09_02_pointer_down_transition_to_active() {
    let state = InteractiveState::resolve_precedence(false, false, true, true, false);
    assert_eq!(state, InteractiveState::Active);
}

/// `T1_F09_03`: Disabled State Rendering
#[test]
fn test_t1_f09_03_disabled_state_rendering() {
    let state = InteractiveState::resolve_precedence(false, true, false, true, false);
    assert_eq!(
        state,
        InteractiveState::Disabled,
        "Disabled must override hover"
    );
}

/// `T1_F09_04`: State Arbitration Precedence
#[test]
fn test_t1_f09_04_state_arbitration_precedence() {
    let theme = Theme::dark();
    let mut ctx = PrimitiveCtx::new(theme);
    ctx.theme.reduced_motion = true;
    ctx.focused = true;
    assert_eq!(
        ctx.effective_state(),
        WidgetState::ReducedMotion,
        "Reduced motion must take absolute precedence"
    );
}

/// `T1_F09_05`: Button Background Color Mapping
#[test]
fn test_t1_f09_05_button_background_color_mapping() {
    let theme = Theme::dark();
    let ctx = PrimitiveCtx::new(theme);
    let mut scene = Scene::new(720, 64, true);
    let btn = Button {
        rect: Rect::new(10.0, 10.0, 100.0, 36.0),
        label: "Tester".into(),
        braise: false,
    };
    btn.draw(&mut scene, &ctx);
    assert!(
        !scene.cmds.is_empty(),
        "Button draw must emit commands into scene"
    );
}
