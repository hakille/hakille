//! Binaire desktop — boucle winit persistante, HUD softbuffer, hotkey global,
//! AccessKit réel. Fenêtre cachée = aucun frame ; rendu logiciel, GPU non requis.
//!
//! Modes : défaut = caché + hotkey global ; `--show` = visible au lancement ;
//! `--authorized` = pont `LocalAuthorized` simulé ; `--smoke` = auto-test
//! (event loop réel + fenêtre cachée + surface softbuffer + 1 rendu) puis sort.
//!
//! Limite documentée : softbuffer 0.4 ne compose pas la transparence (fenêtre
//! noire avec `transparent=true`) → override honnête `transparent=false`.

use accesskit_winit::{
    Adapter as AccessKitAdapter, Event as AccessKitEvent, WindowEvent as AccessKitWindowEvent,
};
use hakille_ipc::transport;
use hakille_ui_desktop::app::{HudAction, HudApp, HudInput};
use hakille_ui_desktop::runtime::WinitConfig;
use hakille_ui_desktop::scene::Scene;
use hakille_ui_desktop::{
    FrameBudget, IpcClient, SceneRenderer, SoftwareRenderer, build_shell_tree,
};
use hakille_ui_platform::hotkey::{HakilleHotkey, HotkeyEvent};
use hakille_ui_protocol::{KernelToUi, UiToKernel};
use std::num::NonZeroU32;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Nœud AccessKit du champ requête (`build_shell_tree`).
const A11Y_QUERY_NODE: accesskit::NodeId = accesskit::NodeId(20);
/// Nœud AccessKit du bouton Valider (`build_shell_tree`).
const A11Y_VALIDATE_NODE: accesskit::NodeId = accesskit::NodeId(22);

#[derive(Debug, Clone, Copy, Default)]
struct Args {
    show: bool,
    authorized: bool,
    smoke: bool,
}

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Args {
    let mut out = Args::default();
    for arg in args {
        match arg.as_str() {
            "--show" => out.show = true,
            "--authorized" => out.authorized = true,
            "--smoke" => out.smoke = true,
            other => eprintln!("hakille-desktop: argument inconnu ignoré: {other}"),
        }
    }
    out
}

/// Événements utilisateur : requêtes AccessKit, appui hotkey et fil IPC
/// (réponses/déconnexion) — tout passe par le thread winit via le proxy.
#[derive(Debug)]
enum AppEvent {
    AccessKit(AccessKitEvent),
    HotkeyToggle,
    /// Réponse du démon reçue par le client IPC.
    KernelReply(KernelToUi),
    /// Handshake IPC réussi : la file de requêtes peut être vidée.
    IpcConnected,
    /// Connexion IPC perdue (le démon s'est arrêté ou a fermé la session).
    IpcDisconnected,
}

impl From<AccessKitEvent> for AppEvent {
    fn from(event: AccessKitEvent) -> Self {
        Self::AccessKit(event)
    }
}

/// Présentation logicielle : contexte, surface, renderer.
struct Present {
    _context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    renderer: SoftwareRenderer,
    size: (u32, u32),
}

impl Present {
    fn new(window: &Arc<Window>) -> Result<Self, String> {
        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| format!("contexte: {e}"))?;
        let mut surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| format!("surface: {e}"))?;
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        surface
            .resize(
                NonZeroU32::new(w).expect("w >= 1"),
                NonZeroU32::new(h).expect("h >= 1"),
            )
            .map_err(|e| format!("resize: {e}"))?;
        Ok(Self {
            _context: context,
            surface,
            renderer: SoftwareRenderer::new(w, h, true),
            size: (w, h),
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        if (w, h) == self.size {
            return;
        }
        let (Some(w), Some(h)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return;
        };
        if let Err(e) = self.surface.resize(w, h) {
            eprintln!("hakille-desktop: resize softbuffer refusé: {e}");
            return;
        }
        self.renderer.begin(w.get(), h.get(), true);
        self.size = (w.get(), h.get());
    }

    /// Peint la scène, copie dans le buffer u32 0x00RRGGBB et présente.
    /// Renvoie `(premier pixel packé, pixels encrés ≠ fond)` — preuve honnête
    /// de rendu réel pour `--smoke` (encrage > 0 ⇒ formes/glyphes peints).
    fn render(&mut self, scene: &Scene) -> Result<(u32, usize), String> {
        let rgba = self.renderer.paint(scene);
        let (bg_r, bg_g, bg_b) = if scene.theme_dark {
            (0x0B, 0x0C, 0x0F)
        } else {
            (0xF6, 0xF4, 0xF0)
        };
        let ink = rgba
            .chunks_exact(4)
            .filter(|c| c[0] != bg_r || c[1] != bg_g || c[2] != bg_b)
            .count();
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| format!("buffer_mut: {e}"))?;
        let len = buffer.len();
        if len == 0 {
            return Err("buffer vide".into());
        }
        let src_w = scene.width.max(1) as usize;
        let src_h = scene.height.max(1) as usize;
        let dst_w = (self.size.0 as usize).clamp(1, len);
        let dst_h = (len / dst_w).max(1);
        // La scène est en 720×64 logiques ; le buffer est en pixels physiques
        // (DPI) → étirement nearest-neighbor, jamais de zone noire.
        let mut i = 0usize;
        for y in 0..dst_h {
            let sy = (y * src_h / dst_h).min(src_h - 1);
            for x in 0..dst_w {
                let sx = (x * src_w / dst_w).min(src_w - 1);
                let p = (sy * src_w + sx) * 4;
                buffer[i] =
                    ((rgba[p] as u32) << 16) | ((rgba[p + 1] as u32) << 8) | (rgba[p + 2] as u32);
                i += 1;
            }
        }
        let first = buffer[0];
        buffer.present().map_err(|e| format!("present: {e}"))?;
        Ok((first, ink))
    }
}

struct WindowState {
    window: Arc<Window>,
    adapter: AccessKitAdapter,
    present: Present,
}

impl WindowState {
    /// Arbre AccessKit complet, focus sur le champ requête si HUD visible.
    fn tree(&self, core: &HudApp) -> accesskit::TreeUpdate {
        let mut tree = build_shell_tree(&core.snapshot(), &[], &[]);
        if core.visible {
            tree.focus = A11Y_QUERY_NODE;
        }
        tree
    }
}

struct DesktopApp {
    args: Args,
    core: HudApp,
    proxy: EventLoopProxy<AppEvent>,
    /// Garde vivant l'enregistrement OS du hotkey (jamais lu directement ;
    /// détruit = enregistrement relâché). Le canal est transféré au thread
    /// relais (voir `main`).
    _hotkey: Option<HakilleHotkey>,
    /// Client IPC du démon (`None` : `--authorized` simulation et `--smoke`
    /// test pur restent sans fil réel).
    ipc: Option<IpcClient>,
    window: Option<WindowState>,
    pending_frame_at: Option<Instant>,
    frames_rendered: u64,
    code: u8,
    hotkey_status: String,
    last_present_error: Option<String>,
}

impl DesktopApp {
    fn create_window(
        &self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(Arc<Window>, AccessKitAdapter), String> {
        // Transparence désactivée : softbuffer ne compose pas l'alpha (noir).
        let config = WinitConfig {
            transparent: false,
            ..WinitConfig::default()
        };
        let attributes = config
            .window_attributes()
            .with_resizable(false)
            .with_visible(false); // AccessKit exige une fenêtre invisible à la création.
        let window = event_loop
            .create_window(attributes)
            .map_err(|e| e.to_string())?;
        let window = Arc::new(window);
        let adapter =
            AccessKitAdapter::with_event_loop_proxy(event_loop, &window, self.proxy.clone());
        Ok((window, adapter))
    }

    fn show_window_ui(&self) {
        if let Some(ws) = self.window.as_ref() {
            ws.window.set_visible(true);
            ws.window.set_ime_allowed(true);
            ws.window.focus_window();
        }
    }

    /// Applique les effets de la machine à états.
    fn apply(&mut self, actions: Vec<HudAction>, event_loop: &ActiveEventLoop) {
        for action in actions {
            match action {
                HudAction::ShowWindow => {
                    self.show_window_ui();
                    self.pending_frame_at = None;
                }
                HudAction::HideWindow => {
                    if let Some(ws) = self.window.as_ref() {
                        ws.window.set_visible(false);
                    }
                    self.pending_frame_at = None;
                }
                HudAction::Redraw => {
                    if self.core.visible
                        && let Some(ws) = self.window.as_ref()
                    {
                        ws.window.request_redraw();
                    }
                }
                HudAction::RequestFrameIn(delay) => {
                    let at = Instant::now() + delay;
                    self.pending_frame_at = Some(match self.pending_frame_at {
                        Some(existing) if existing < at => existing,
                        _ => at,
                    });
                }
                HudAction::ResizeWindow { width, height } => {
                    if let Some(ws) = self.window.as_ref() {
                        let _ = ws
                            .window
                            .request_inner_size(LogicalSize::new(width as f64, height as f64));
                    }
                }
                HudAction::Submit { query } => {
                    // Démon branché : envoi IPC (submit non bloquant) puis état
                    // « en attente ». Démon pas encore prêt : la requête est
                    // mise en file et partira au handshake, jamais perdue.
                    let ipc_present = self.ipc.is_some();
                    let sent = self.ipc.as_ref().is_some_and(|ipc| {
                        ipc.connected()
                            && ipc.submit(UiToKernel::HudSubmit {
                                query: query.clone(),
                            })
                    });
                    let actions = if sent {
                        self.core.on_submit_sent(query)
                    } else if ipc_present {
                        self.core.on_query_queued(query)
                    } else {
                        self.core.submit_query(query)
                    };
                    self.apply(actions, event_loop);
                }
                HudAction::RequestJournal { limit } => {
                    let sent = self
                        .ipc
                        .as_ref()
                        .is_some_and(|ipc| ipc.request_journal(limit));
                    if !sent {
                        let actions = self.core.on_ipc_unavailable("journal");
                        self.apply(actions, event_loop);
                    }
                }
                HudAction::RequestMemory { query, limit } => {
                    let sent = self
                        .ipc
                        .as_ref()
                        .is_some_and(|ipc| ipc.request_memory(query, limit));
                    if !sent {
                        let actions = self.core.on_ipc_unavailable("mémoire");
                        self.apply(actions, event_loop);
                    }
                }
                HudAction::RequestStatus => {
                    let sent = self.ipc.as_ref().is_some_and(|ipc| ipc.request_status());
                    if !sent {
                        let actions = self.core.on_ipc_unavailable("statut");
                        self.apply(actions, event_loop);
                    }
                }
                HudAction::Quit => event_loop.exit(),
            }
        }
    }

    fn redraw(&mut self) {
        let Some(ws) = self.window.as_mut() else {
            return;
        };
        let tree = ws.tree(&self.core);
        ws.adapter.update_if_active(move || tree);
        let scene = self.core.scene();
        match ws.present.render(&scene) {
            Ok(_) => self.frames_rendered += 1,
            Err(e) => {
                eprintln!("hakille-desktop: rendu échoué: {e}");
                self.last_present_error = Some(e);
            }
        }
    }

    /// Auto-test : fenêtre cachée + surface + 1 rendu → lignes SMOKE → sortie.
    fn run_smoke(&mut self, event_loop: &ActiveEventLoop) {
        let Some(ws) = self.window.as_mut() else {
            eprintln!("SMOKE window=err(fenêtre absente après resumed)");
            self.code = 2;
            event_loop.exit();
            return;
        };
        let tree = ws.tree(&self.core);
        ws.adapter.update_if_active(move || tree);
        let scene = self.core.scene();
        match ws.present.render(&scene) {
            Ok((first_pixel, ink)) => {
                self.frames_rendered = 1;
                println!(
                    "SMOKE window=ok softbuffer=ok hotkey={} frames={} first_pixel=0x{:06X} ink={}",
                    self.hotkey_status, self.frames_rendered, first_pixel, ink
                );
                println!(
                    "SMOKE hotkey_default_requested={} (repli Alt+Space possible, non exposé par le contrat)",
                    hakille_ui_platform::hotkey::default_hotkey_label()
                );
                println!(
                    "SMOKE note=transparent=false (limite softbuffer 0.4, jamais de noir silencieux)"
                );
                self.code = 0;
            }
            Err(e) => {
                eprintln!(
                    "SMOKE window=ok softbuffer=ok hotkey={} render=err({e})",
                    self.hotkey_status
                );
                self.code = 4;
            }
        }
        event_loop.exit();
    }
}

impl ApplicationHandler<AppEvent> for DesktopApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let (window, adapter) = match self.create_window(event_loop) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("hakille-desktop: fenêtre impossible: {e}");
                    if self.args.smoke {
                        eprintln!("SMOKE window=err({e})");
                    }
                    self.code = 2;
                    event_loop.exit();
                    return;
                }
            };
            let present = match Present::new(&window) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("hakille-desktop: surface logicielle impossible: {e}");
                    if self.args.smoke {
                        eprintln!("SMOKE window=ok softbuffer=err({e})");
                    }
                    self.code = 3;
                    event_loop.exit();
                    return;
                }
            };
            self.window = Some(WindowState {
                window,
                adapter,
                present,
            });
            if self.core.visible && !self.args.smoke {
                self.show_window_ui();
                if let Some(ws) = self.window.as_ref() {
                    ws.window.request_redraw();
                }
            }
        }
        if self.args.smoke {
            self.run_smoke(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::HotkeyToggle => {
                let actions = self.core.handle(HudInput::HotkeyToggle);
                self.apply(actions, event_loop);
            }
            AppEvent::KernelReply(msg) => {
                let actions = self.core.on_kernel_reply(msg);
                self.apply(actions, event_loop);
            }
            AppEvent::IpcConnected => {
                let actions = self.core.on_ipc_connected();
                self.apply(actions, event_loop);
            }
            AppEvent::IpcDisconnected => {
                let actions = self.core.on_kernel_disconnected();
                self.apply(actions, event_loop);
            }
            AppEvent::AccessKit(event) => match event.window_event {
                AccessKitWindowEvent::InitialTreeRequested => {
                    if let Some(ws) = self.window.as_mut() {
                        let tree = ws.tree(&self.core);
                        ws.adapter.update_if_active(move || tree);
                    }
                }
                AccessKitWindowEvent::ActionRequested(request) => {
                    // Focus/click sur le champ requête (ou la présence) → HUD.
                    let input = if request.target_node == A11Y_VALIDATE_NODE {
                        HudInput::Enter
                    } else {
                        HudInput::FocusQuery
                    };
                    if request.target_node == A11Y_QUERY_NODE
                        || request.target_node == A11Y_VALIDATE_NODE
                        || request.target_node == accesskit::NodeId(1)
                    {
                        let actions = self.core.handle(input);
                        self.apply(actions, event_loop);
                    }
                }
                AccessKitWindowEvent::AccessibilityDeactivated => {}
            },
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(ws) = self.window.as_mut() else {
            return;
        };
        if ws.window.id() != id {
            return;
        }
        ws.adapter.process_event(&ws.window, &event);

        let mut inputs: Vec<HudInput> = Vec::new();
        match event {
            WindowEvent::CloseRequested => inputs.push(HudInput::Close),
            WindowEvent::Resized(size) => {
                if let Some(ws) = self.window.as_mut() {
                    ws.present.resize(size.width, size.height);
                    if self.core.visible {
                        ws.window.request_redraw();
                    }
                }
                return;
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                return;
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        text,
                        ..
                    },
                ..
            } => match &logical_key {
                Key::Named(NamedKey::Escape) => inputs.push(HudInput::Escape),
                Key::Named(NamedKey::Enter) => inputs.push(HudInput::Enter),
                Key::Named(NamedKey::Backspace) => inputs.push(HudInput::Backspace),
                Key::Named(NamedKey::F1) => inputs.push(HudInput::ShowJournal),
                Key::Named(NamedKey::F2) => inputs.push(HudInput::ShowMemory),
                Key::Named(NamedKey::F3) => inputs.push(HudInput::ShowStatus),
                _ => {
                    if let Some(text) = text {
                        for c in text.chars() {
                            if !c.is_control() {
                                inputs.push(HudInput::Char(c));
                            }
                        }
                    }
                }
            },
            WindowEvent::Ime(Ime::Commit(text)) => inputs.push(HudInput::ImeCommit(text)),
            // Pas de préédition affichée (limite documentée : IME simple seulement).
            WindowEvent::Ime(Ime::Preedit(..)) => {}
            _ => {}
        }
        for input in inputs {
            let actions = self.core.handle(input);
            self.apply(actions, event_loop);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if let Some(at) = self.pending_frame_at
            && now >= at
        {
            self.pending_frame_at = None;
            let actions = self.core.handle(HudInput::Tick);
            self.apply(actions, event_loop);
        }
        if self.pending_frame_at.is_none() && self.core.needs_animation() {
            self.pending_frame_at = Some(Instant::now() + self.core.frame_interval());
        }
        match self.pending_frame_at {
            Some(at) => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
            // Veille réelle (0 CPU) : le réveil hotkey passe par le thread
            // relais + `EventLoopProxy`, pas par la pompe de la boucle.
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {}
}
fn main() -> ExitCode {
    let args = parse_args(std::env::args().skip(1));

    let event_loop = match EventLoop::<AppEvent>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("hakille-desktop: event loop impossible: {e}");
            return ExitCode::from(1);
        }
    };
    let proxy = event_loop.create_proxy();

    // Hotkey global : le manager (non `Send`) reste vivant dans l'app, sur le
    // thread principal ; le canal part dans un thread relais qui réveille la
    // boucle via le proxy — global-hotkey livre depuis son propre thread, une
    // boucle endormie (`ControlFlow::Wait`) ne serait jamais réveillée.
    let (hotkey_status, mut hotkey) = match HakilleHotkey::register_default() {
        Ok(hotkey) => (String::from("ok"), Some(hotkey)),
        Err(e) => {
            eprintln!(
                "avertissement: hotkey global indisponible ({e}) — démarrage visible en repli"
            );
            (format!("unavailable({e})"), None)
        }
    };

    if let Some(rx) = hotkey.as_mut().and_then(HakilleHotkey::take_receiver) {
        let relay_proxy = proxy.clone();
        std::thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                match event {
                    HotkeyEvent::ToggleHud => {
                        if relay_proxy.send_event(AppEvent::HotkeyToggle).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }

    // Client IPC : thread dédié (runtime tokio mono-thread), réponses et
    // déconnexions renvoyées à la boucle via le proxy — jamais de blocage du
    // thread winit. `--authorized` (simulation) et `--smoke` (test pur)
    // restent sans fil réel.
    let ipc = if args.authorized || args.smoke {
        None
    } else {
        let connected_proxy = proxy.clone();
        let reply_proxy = proxy.clone();
        let disconnect_proxy = proxy.clone();
        Some(IpcClient::spawn_with_connected(
            transport::default_endpoint(),
            transport::default_token_path(),
            move || {
                let _ = connected_proxy.send_event(AppEvent::IpcConnected);
            },
            move |msg| {
                let _ = reply_proxy.send_event(AppEvent::KernelReply(msg));
            },
            move || {
                let _ = disconnect_proxy.send_event(AppEvent::IpcDisconnected);
            },
        ))
    };

    let start_visible = args.show || hotkey.is_none();
    let mut app = DesktopApp {
        args,
        core: HudApp::new(
            args.authorized,
            start_visible,
            FrameBudget::battery_saver(),
            false,
        ),
        proxy,
        _hotkey: hotkey,
        ipc,
        window: None,
        pending_frame_at: None,
        frames_rendered: 0,
        code: 0,
        hotkey_status,
        last_present_error: None,
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("hakille-desktop: boucle événements: {e}");
        return ExitCode::from(1);
    }
    if !app.args.smoke {
        println!("frames={}", app.frames_rendered);
        if let Some(err) = &app.last_present_error {
            eprintln!("hakille-desktop: dernière erreur de présentation: {err}");
        }
    }
    ExitCode::from(app.code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_recognizes_contract_flags() {
        let args = parse_args(
            ["--show", "--authorized", "--smoke"]
                .iter()
                .map(|s| s.to_string()),
        );
        assert!(args.show && args.authorized && args.smoke);
        let empty = parse_args(std::iter::empty::<String>());
        assert!(!empty.show && !empty.authorized && !empty.smoke);
    }
}
