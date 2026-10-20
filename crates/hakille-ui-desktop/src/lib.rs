//! Shell desktop natif Hakille — winit + wgpu + AccessKit.
//!
//! Architecture (plan refonte §3) :
//! ```text
//! noyau → protocol → model (pur) → desktop (ce crate)
//!   runtime : winit + boucle pilotée événements, 0 frame en veille
//!   renderer : SceneRenderer (wgpu réel / software fallback explicite)
//!   texte : TextEngine (shaping, RTL, IME, fallback polices)
//!   a11y : AccessKit, même hiérarchie que l'affichage
//! ```
//!
//! Règles : renderer = view-models immuables ; interactions = UiEvent ;
//! fenêtre masquée = aucun frame ; animation ≤30fps batterie, arrêt si
//! mouvement réduit ; GPU absent = fallback logiciel ou erreur explicite.

pub mod a11y;
pub mod app;
pub mod components;
pub mod daemon_bridge;
pub mod i18n;
pub mod ipc_client;
pub mod kernel_bridge;
pub mod renderer;
pub mod runtime;
pub mod scene;
pub mod text;
pub mod theme;

pub use a11y::{audit_interactive_labels, build_action_card_tree, build_shell_tree};
pub use app::{AuxNotice, HudAction, HudApp, HudInput};
pub use components::*;
pub use daemon_bridge::{AuthorityPolicy, DaemonKernel, RealDaemonBridge};
pub use i18n::{Locale, Strings};
pub use ipc_client::IpcClient;
pub use kernel_bridge::{KernelBridge, KernelStub};
pub use renderer::{GpuStatus, Renderer, SoftwareRenderer, WgpuRenderer};
pub use runtime::{FrameBudget, HeadlessRuntime, RuntimeEvent};
pub use scene::{Color, Rect, Scene, SceneRenderer};
pub use text::{ShapedRun, SimpleTextEngine, TextEngine};
pub use theme::{Density, TextScale, Theme, ThemeMode};
