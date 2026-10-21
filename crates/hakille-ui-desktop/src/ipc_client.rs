//! Client IPC du HUD desktop — un thread dédié, zéro blocage du thread winit.
//!
//! `submit` ne fait qu'un `send` de canal (rapide, jamais bloquant) : l'E/S
//! vit dans un thread avec runtime tokio `current_thread`. Connexion,
//! handshake puis lecture des trames se font là-bas ; chaque réponse est
//! renvoyée à l'hôte via le callback `on_reply` (proxy d'événements winit).
//!
//! Reconnexion infinie : le démon peut démarrer après le HUD (`sleep 2 s`
//! entre chaque tentative ratée). Aucun succès simulé : si le démon est
//! absent, `submit` renvoie `false` et l'hôte applique son repli fail-closed.

use hakille_ipc::frame::{Frame, FrameBody};
use hakille_ipc::transport;
use hakille_ui_protocol::{KernelToUi, UiToKernel};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::mpsc;

/// Délai entre deux tentatives de connexion (le démon peut démarrer après).
const RETRY_DELAY: Duration = Duration::from_secs(2);

/// Client IPC local du HUD.
pub struct IpcClient {
    outbound: mpsc::UnboundedSender<Frame>,
    connected: Arc<AtomicBool>,
    next_id: AtomicU64,
    _thread: JoinHandle<()>,
}

impl IpcClient {
    /// Endpoint et jeton par défaut (`hakille_ipc::transport`) : env
    /// `HAKILLE_IPC_ENDPOINT`/`HAKILLE_IPC_TOKEN_FILE` sinon pipe/socket local.
    pub fn spawn(
        on_reply: impl Fn(KernelToUi) + Send + 'static,
        on_disconnected: impl Fn() + Send + 'static,
    ) -> Self {
        Self::spawn_with(
            transport::default_endpoint(),
            transport::default_token_path(),
            on_reply,
            on_disconnected,
        )
    }

    /// Variante testable : endpoint et chemin de jeton explicites.
    pub fn spawn_with(
        endpoint: String,
        token_path: PathBuf,
        on_reply: impl Fn(KernelToUi) + Send + 'static,
        on_disconnected: impl Fn() + Send + 'static,
    ) -> Self {
        Self::spawn_with_connected(endpoint, token_path, || {}, on_reply, on_disconnected)
    }

    /// Variante avec callback de connexion (handshake réussi) : l'hôte peut
    /// alors vider sa file de requêtes saisies avant le démarrage du démon.
    pub fn spawn_with_connected(
        endpoint: String,
        token_path: PathBuf,
        on_connected: impl Fn() + Send + 'static,
        on_reply: impl Fn(KernelToUi) + Send + 'static,
        on_disconnected: impl Fn() + Send + 'static,
    ) -> Self {
        let (outbound, receiver) = mpsc::unbounded_channel();
        let connected = Arc::new(AtomicBool::new(false));
        let thread_connected = Arc::clone(&connected);
        let _thread = std::thread::Builder::new()
            .name("hakille-ipc-client".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        tracing::warn!(error = %err, "runtime IPC impossible");
                        return;
                    }
                };
                runtime.block_on(run_client(
                    endpoint,
                    token_path,
                    receiver,
                    thread_connected,
                    on_connected,
                    on_reply,
                    on_disconnected,
                ));
            })
            .expect("thread IPC hakille");
        Self {
            outbound,
            connected,
            next_id: AtomicU64::new(1),
            _thread,
        }
    }

    /// `true` ssi le handshake a réussi et la connexion est vivante.
    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Enfile une requête (id monotone ≥ 1). `false` si non connecté :
    /// l'hôte garde alors son repli fail-closed, jamais de succès feint.
    pub fn submit(&self, message: UiToKernel) -> bool {
        if !self.connected() {
            return false;
        }
        let frame = Frame::request(self.next_id.fetch_add(1, Ordering::Relaxed), message);
        self.outbound.send(frame).is_ok()
    }

    /// Consultation additive v1 : `limit` dernières entrées d'audit.
    pub fn request_journal(&self, limit: usize) -> bool {
        self.submit(UiToKernel::JournalRequest { limit })
    }

    /// Consultation additive v1 : vue mémoire (`query: None`) ou rappel
    /// (`Some`) — `limit` lignes maximum.
    pub fn request_memory(&self, query: Option<String>, limit: usize) -> bool {
        self.submit(UiToKernel::MemoryRequest { query, limit })
    }

    /// Mémorisation additive v1 d'un fait en texte libre (accusé = id réel).
    pub fn remember(&self, text: impl Into<String>) -> bool {
        self.submit(UiToKernel::MemoryRemember { text: text.into() })
    }

    /// Statut minimal du démon (modèle, coût, état).
    pub fn request_status(&self) -> bool {
        self.submit(UiToKernel::StatusRequest)
    }
}

/// Fin de session côté client.
enum SessionEnd {
    /// Erreur d'E/S, EOF ou écriture refusée : retour boucle externe.
    Disconnected,
    /// Client jeté (canal fermé) : le thread s'arrête.
    Shutdown,
}

/// Boucle de vie : jeton → connexion → handshake → session → reconnexion.
/// Sans fin tant que le client vit (le démon peut démarrer après le HUD).
async fn run_client(
    endpoint: String,
    token_path: PathBuf,
    mut outbound: mpsc::UnboundedReceiver<Frame>,
    connected: Arc<AtomicBool>,
    on_connected: impl Fn() + Send + 'static,
    on_reply: impl Fn(KernelToUi) + Send + 'static,
    on_disconnected: impl Fn() + Send + 'static,
) {
    loop {
        if outbound.is_closed() {
            return;
        }
        let token = match transport::read_token_file(&token_path) {
            Ok(token) if !token.is_empty() => token,
            _ => {
                tokio::time::sleep(RETRY_DELAY).await;
                continue;
            }
        };
        let mut stream = match transport::connect(&endpoint).await {
            Ok(stream) => stream,
            Err(_) => {
                tokio::time::sleep(RETRY_DELAY).await;
                continue;
            }
        };
        if transport::client_handshake(&mut stream, &token)
            .await
            .is_err()
        {
            tokio::time::sleep(RETRY_DELAY).await;
            continue;
        }
        connected.store(true, Ordering::SeqCst);
        on_connected();
        let end = run_session(stream, &mut outbound, &on_reply).await;
        connected.store(false, Ordering::SeqCst);
        match end {
            SessionEnd::Disconnected => on_disconnected(),
            SessionEnd::Shutdown => return,
        }
    }
}

/// Une session connectée : écrit les trames sortantes, distribue les entrantes.
///
/// La lecture vit dans une tâche dédiée : `read_frame` n'est pas
/// annulation-sûre (une trame à moitié lue serait perdue), donc le
/// `tokio::select!` principal ne manie que des `recv` de canaux, eux
/// annulation-sûrs.
async fn run_session(
    stream: transport::ClientStream,
    outbound: &mut mpsc::UnboundedReceiver<Frame>,
    on_reply: &impl Fn(KernelToUi),
) -> SessionEnd {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (frames_tx, mut frames_rx) = mpsc::unbounded_channel::<Frame>();
    let reader_task = tokio::spawn(async move {
        loop {
            match transport::read_frame(&mut reader).await {
                Ok(Some(frame)) => {
                    if frames_tx.send(frame).is_err() {
                        return;
                    }
                }
                Ok(None) | Err(_) => return,
            }
        }
    });
    let end = loop {
        tokio::select! {
            maybe = outbound.recv() => {
                let Some(frame) = maybe else {
                    break SessionEnd::Shutdown;
                };
                if transport::write_frame(&mut writer, &frame).await.is_err() {
                    break SessionEnd::Disconnected;
                }
            }
            frame = frames_rx.recv() => {
                match frame {
                    Some(frame) => deliver(frame, on_reply),
                    None => break SessionEnd::Disconnected,
                }
            }
        }
    };
    reader_task.abort();
    end
}

/// Distribue une trame entrante au HUD : `Reply` → message, `Error` → toast
/// honnête `démon: <message> (<code>)`, les autres trames sont ignorées.
fn deliver(frame: Frame, on_reply: &impl Fn(KernelToUi)) {
    match frame.body {
        FrameBody::Reply { message } => on_reply(message),
        FrameBody::Error { code, message } => on_reply(KernelToUi::Toast {
            text: format!("démon: {message} ({code:?})"),
        }),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_ipc::transport::{
        IpcListener, read_frame, server_handshake, write_frame, write_token_file,
    };
    use std::time::Instant;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn unique_id() -> u64 {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    fn unique_endpoint(tag: &str) -> String {
        let id = unique_id();
        let pid = std::process::id();
        #[cfg(windows)]
        {
            format!(r"\\.\pipe\hakille-ui-desktop-ipc-{tag}-{pid}-{id}")
        }
        #[cfg(unix)]
        {
            format!("/tmp/hakille-ui-desktop-ipc-{tag}-{pid}-{id}.sock")
        }
    }

    fn unique_temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "hakille-ui-desktop-ipc-{tag}-{}-{}",
            std::process::id(),
            unique_id()
        ))
    }

    fn cleanup_endpoint(endpoint: &str) {
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(endpoint);
        }
        #[cfg(not(unix))]
        {
            let _ = endpoint;
        }
    }

    #[test]
    fn roundtrip_with_real_listener() {
        let endpoint = unique_endpoint("roundtrip");
        let dir = unique_temp_dir("roundtrip");
        let token_path = dir.join("ipc.token");
        write_token_file(&token_path, TOKEN).expect("écriture jeton");

        let server_endpoint = endpoint.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime serveur");
            runtime.block_on(async move {
                let listener = IpcListener::bind(&server_endpoint).await.expect("bind");
                ready_tx.send(()).expect("signal serveur prêt");
                let mut stream = listener.accept().await.expect("accept");
                server_handshake(&mut stream, TOKEN)
                    .await
                    .expect("handshake serveur");
                let request = read_frame(&mut stream)
                    .await
                    .expect("lecture requête")
                    .expect("trame requête");
                assert!(matches!(&request.body, FrameBody::Request { .. }));
                let reply = Frame::reply(
                    request.id,
                    KernelToUi::HudReply {
                        response: "pong".to_string(),
                        validation: None,
                    },
                );
                write_frame(&mut stream, &reply)
                    .await
                    .expect("écriture réponse");
            });
        });
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("serveur prêt");

        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        let (gone_tx, _gone_rx) = std::sync::mpsc::channel();
        let client = IpcClient::spawn_with(
            endpoint.clone(),
            token_path,
            move |msg| {
                let _ = reply_tx.send(msg);
            },
            move || {
                let _ = gone_tx.send(());
            },
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        while !client.connected() {
            assert!(Instant::now() < deadline, "client jamais connecté");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(client.submit(UiToKernel::HudSubmit {
            query: "ping".to_string(),
        }));
        let reply = reply_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("réponse du démon");
        assert_eq!(
            reply,
            KernelToUi::HudReply {
                response: "pong".to_string(),
                validation: None,
            }
        );

        server.join().expect("thread serveur");
        drop(client);
        let _ = std::fs::remove_dir_all(&dir);
        cleanup_endpoint(&endpoint);
    }

    #[test]
    fn submit_without_daemon_returns_false() {
        let endpoint = unique_endpoint("absent");
        let dir = unique_temp_dir("absent");
        let token_path = dir.join("ipc.token");
        write_token_file(&token_path, TOKEN).expect("écriture jeton");

        let client = IpcClient::spawn_with(endpoint, token_path, |_| {}, || {});
        // Laisse le thread tenter une connexion sur un endpoint inexistant.
        std::thread::sleep(Duration::from_millis(250));
        assert!(!client.connected());
        assert!(!client.submit(UiToKernel::HudSubmit {
            query: "ping".to_string(),
        }));
        drop(client);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
