//! Primitives de gestion de service Linux Systemd.
//!
//! Intègre le protocole `sd_notify` (`READY=1`, `WATCHDOG=1`, `STOPPING=1`),
//! la détection d'activation de socket (`LISTEN_FDS`), et la génération
//! déclarative d'unités de service et de socket pour Systemd.

use serde::{Deserialize, Serialize};
use std::os::unix::io::RawFd;
use thiserror::Error;

/// Descripteur de début pour les sockets passés par systemd (sd_listen_fds).
pub const SD_LISTEN_FDS_START: RawFd = 3;

#[derive(Debug, Error)]
pub enum SystemdError {
    #[error("communication avec le socket de notification systemd échouée: {0}")]
    NotifyFailed(String),
    #[error("format d'adresse de socket invalide: {0}")]
    InvalidSocketAddress(String),
    #[error("erreur d'activation de socket systemd: {0}")]
    SocketActivation(String),
    #[error("erreur I/O lors de la gestion du service: {0}")]
    Io(#[from] std::io::Error),
}

/// États de notification reconnus par systemd (sd_notify).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemdNotifyState<'a> {
    Ready,
    Stopping,
    Watchdog,
    WatchdogTrigger,
    Status(&'a str),
    MainPid(u32),
    Errno(i32),
    Reloading,
    Custom(&'a str),
}

impl<'a> SystemdNotifyState<'a> {
    pub fn as_str(&self) -> String {
        match self {
            Self::Ready => "READY=1".to_string(),
            Self::Stopping => "STOPPING=1".to_string(),
            Self::Watchdog => "WATCHDOG=1".to_string(),
            Self::WatchdogTrigger => "WATCHDOG=trigger".to_string(),
            Self::Status(s) => format!("STATUS={s}"),
            Self::MainPid(pid) => format!("MAINPID={pid}"),
            Self::Errno(e) => format!("ERRNO={e}"),
            Self::Reloading => "RELOADING=1".to_string(),
            Self::Custom(c) => c.to_string(),
        }
    }
}

/// Envoie une notification à systemd via le socket défini dans `NOTIFY_SOCKET`.
///
/// Renvoie `Ok(true)` si la notification a été transmise, `Ok(false)` si
/// `NOTIFY_SOCKET` n'est pas défini (exécution normale hors systemd).
pub fn notify(state: &SystemdNotifyState<'_>) -> Result<bool, SystemdError> {
    let notify_socket = match std::env::var("NOTIFY_SOCKET") {
        Ok(s) if !s.trim().is_empty() => s,
        _ => return Ok(false),
    };
    notify_to_socket(&notify_socket, state)?;
    Ok(true)
}

/// Envoie une notification à un chemin de socket datagram Unix ou abstrait spécifique.
pub fn notify_to_socket(
    socket_path: &str,
    state: &SystemdNotifyState<'_>,
) -> Result<(), SystemdError> {
    use std::os::unix::net::{SocketAddr, UnixDatagram};

    let sock = UnixDatagram::unbound().map_err(SystemdError::Io)?;

    let addr = if let Some(abstract_name) = socket_path.strip_prefix('@') {
        #[cfg(target_os = "linux")]
        {
            use std::os::linux::net::SocketAddrExt;
            SocketAddr::from_abstract_name(abstract_name.as_bytes())
                .map_err(|e| SystemdError::InvalidSocketAddress(e.to_string()))?
        }
        #[cfg(not(target_os = "linux"))]
        {
            return Err(SystemdError::InvalidSocketAddress(
                "Les sockets abstraits (@) sont uniquement pris en charge sous Linux".to_string(),
            ));
        }
    } else {
        SocketAddr::from_pathname(socket_path)
            .map_err(|e| SystemdError::InvalidSocketAddress(e.to_string()))?
    };

    let msg = format!("{}\n", state.as_str());
    sock.connect_addr(&addr)
        .map_err(|e| SystemdError::NotifyFailed(e.to_string()))?;
    sock.send(msg.as_bytes())
        .map_err(|e| SystemdError::NotifyFailed(e.to_string()))?;

    Ok(())
}

/// Gestionnaire de pulsation Watchdog pour systemd (`WATCHDOG_USEC`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemdWatchdog {
    interval: std::time::Duration,
    recommended_ping_interval: std::time::Duration,
}

impl SystemdWatchdog {
    /// Initialise le watchdog en lisant les variables d'environnement `WATCHDOG_USEC`
    /// et optionnellement `WATCHDOG_PID`.
    pub fn from_env() -> Option<Self> {
        let usec_str = std::env::var("WATCHDOG_USEC").ok()?;
        let usec: u64 = usec_str.parse().ok()?;
        if usec == 0 {
            return None;
        }
        if let Ok(pid_str) = std::env::var("WATCHDOG_PID") {
            if let Ok(target_pid) = pid_str.parse::<u32>() {
                if target_pid != std::process::id() {
                    return None;
                }
            }
        }
        let interval = std::time::Duration::from_micros(usec);
        let recommended_ping_interval = interval / 2;
        Some(Self {
            interval,
            recommended_ping_interval,
        })
    }

    pub fn from_duration(interval: std::time::Duration) -> Self {
        let recommended_ping_interval = interval / 2;
        Self {
            interval,
            recommended_ping_interval,
        }
    }

    pub fn interval(&self) -> std::time::Duration {
        self.interval
    }

    pub fn recommended_ping_interval(&self) -> std::time::Duration {
        self.recommended_ping_interval
    }

    pub fn ping(&self) -> Result<bool, SystemdError> {
        notify(&SystemdNotifyState::Watchdog)
    }
}

/// Socket activé par systemd (sd_listen_fds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivatedSocket {
    pub fd: RawFd,
    pub name: Option<String>,
}

impl ActivatedSocket {
    pub fn new(fd: RawFd, name: Option<String>) -> Self {
        Self { fd, name }
    }

    /// Convertit le descripteur brut en `std::os::unix::net::UnixListener`.
    ///
    /// # Safety
    /// Prend possession du descripteur de fichier ; ne doit être appelé qu'une seule fois.
    pub unsafe fn into_unix_listener(self) -> std::io::Result<std::os::unix::net::UnixListener> {
        use std::os::unix::io::FromRawFd;
        Ok(unsafe { std::os::unix::net::UnixListener::from_raw_fd(self.fd) })
    }

    /// Convertit le descripteur brut en `tokio::net::UnixListener`.
    ///
    /// # Safety
    /// Prend possession du descripteur de fichier.
    pub unsafe fn into_tokio_unix_listener(self) -> std::io::Result<tokio::net::UnixListener> {
        let std_listener = unsafe { self.into_unix_listener()? };
        std_listener.set_nonblocking(true)?;
        tokio::net::UnixListener::from_std(std_listener)
    }
}

/// Analyse déterministe des descripteurs de socket activation à partir de valeurs de variables.
pub fn parse_listen_fds_from(
    fds_str: Option<&str>,
    pid_str: Option<&str>,
    fdnames_str: Option<&str>,
    current_pid: u32,
) -> Vec<ActivatedSocket> {
    if let Some(target_pid_str) = pid_str {
        if let Ok(target_pid) = target_pid_str.trim().parse::<u32>() {
            if target_pid != current_pid {
                return Vec::new();
            }
        }
    }

    let fds_count: usize = match fds_str {
        Some(s) => match s.trim().parse() {
            Ok(n) if n > 0 => n,
            _ => return Vec::new(),
        },
        None => return Vec::new(),
    };

    let names: Vec<String> = match fdnames_str {
        Some(s) if !s.trim().is_empty() => s.split(':').map(|n| n.trim().to_string()).collect(),
        _ => Vec::new(),
    };

    let mut sockets = Vec::with_capacity(fds_count);
    for i in 0..fds_count {
        let fd = SD_LISTEN_FDS_START + i as RawFd;
        let name = names.get(i).cloned();
        sockets.push(ActivatedSocket { fd, name });
    }

    sockets
}

/// Vérifie l'activation de sockets via `LISTEN_FDS`, `LISTEN_PID` et `LISTEN_FDNAMES`.
pub fn listen_fds(unset_env: bool) -> Result<Vec<ActivatedSocket>, SystemdError> {
    let fds_var = std::env::var("LISTEN_FDS").ok();
    let pid_var = std::env::var("LISTEN_PID").ok();
    let fdnames_var = std::env::var("LISTEN_FDNAMES").ok();

    let sockets = parse_listen_fds_from(
        fds_var.as_deref(),
        pid_var.as_deref(),
        fdnames_var.as_deref(),
        std::process::id(),
    );

    if unset_env {
        unsafe {
            std::env::remove_var("LISTEN_FDS");
            std::env::remove_var("LISTEN_PID");
            std::env::remove_var("LISTEN_FDNAMES");
        }
    }

    Ok(sockets)
}

/// Portée de l'unité Systemd (système global ou session utilisateur).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemdUnitScope {
    System,
    User,
}

/// Configuration déclarative d'une unité de service Systemd (`.service`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdServiceUnitConfig {
    pub scope: SystemdUnitScope,
    pub service_name: String,
    pub description: String,
    pub documentation: Vec<String>,
    pub exec_start: String,
    pub working_directory: Option<String>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub restart: String,
    pub restart_sec: u32,
    pub watchdog_sec: Option<u32>,
    pub memory_max: Option<String>,
    pub tasks_max: Option<u32>,
    pub protect_system: String,
    pub private_tmp: bool,
    pub private_devices: bool,
    pub restrict_address_families: Vec<String>,
}

impl SystemdServiceUnitConfig {
    pub fn default_system(exec_path: &str) -> Self {
        Self {
            scope: SystemdUnitScope::System,
            service_name: "hakilled".to_string(),
            description: "Hakille Sovereign Cognitive OS Daemon".to_string(),
            documentation: vec!["https://github.com/hakille/hakille".to_string()],
            exec_start: format!("{exec_path} --serve"),
            working_directory: Some("/var/lib/hakille".to_string()),
            user: Some("hakille".to_string()),
            group: Some("hakille".to_string()),
            restart: "on-failure".to_string(),
            restart_sec: 5,
            watchdog_sec: Some(30),
            memory_max: Some("4G".to_string()),
            tasks_max: Some(4096),
            protect_system: "strict".to_string(),
            private_tmp: true,
            private_devices: true,
            restrict_address_families: vec![
                "AF_UNIX".to_string(),
                "AF_INET".to_string(),
                "AF_INET6".to_string(),
            ],
        }
    }

    pub fn default_user(exec_path: &str) -> Self {
        Self {
            scope: SystemdUnitScope::User,
            service_name: "hakilled".to_string(),
            description: "Hakille Sovereign Cognitive OS Daemon (User Session)".to_string(),
            documentation: vec!["https://github.com/hakille/hakille".to_string()],
            exec_start: format!("{exec_path} --serve"),
            working_directory: None,
            user: None,
            group: None,
            restart: "on-failure".to_string(),
            restart_sec: 3,
            watchdog_sec: Some(30),
            memory_max: Some("2G".to_string()),
            tasks_max: Some(1024),
            protect_system: "true".to_string(),
            private_tmp: true,
            private_devices: false,
            restrict_address_families: vec![
                "AF_UNIX".to_string(),
                "AF_INET".to_string(),
                "AF_INET6".to_string(),
            ],
        }
    }

    /// Génère le contenu au format INI pour le fichier `.service`.
    pub fn render_unit(&self) -> String {
        let mut out = String::new();
        out.push_str("[Unit]\n");
        out.push_str(&format!("Description={}\n", self.description));
        for doc in &self.documentation {
            out.push_str(&format!("Documentation={}\n", doc));
        }
        out.push_str("After=network.target\n");
        out.push_str(&format!("Wants={}.socket\n\n", self.service_name));

        out.push_str("[Service]\n");
        out.push_str("Type=notify\n");
        out.push_str("NotifyAccess=main\n");
        out.push_str(&format!("ExecStart={}\n", self.exec_start));
        out.push_str(&format!("Restart={}\n", self.restart));
        out.push_str(&format!("RestartSec={}s\n", self.restart_sec));

        if let Some(wd) = self.watchdog_sec {
            out.push_str(&format!("WatchdogSec={}s\n", wd));
        }

        if let Some(dir) = &self.working_directory {
            out.push_str(&format!("WorkingDirectory={}\n", dir));
        }
        if let Some(user) = &self.user {
            out.push_str(&format!("User={}\n", user));
        }
        if let Some(group) = &self.group {
            out.push_str(&format!("Group={}\n", group));
        }

        if self.scope == SystemdUnitScope::System {
            out.push_str("RuntimeDirectory=hakille\n");
            out.push_str("StateDirectory=hakille\n");
            out.push_str("ConfigurationDirectory=hakille\n");
        }

        out.push_str(&format!("ProtectSystem={}\n", self.protect_system));
        out.push_str(&format!(
            "PrivateTmp={}\n",
            if self.private_tmp { "true" } else { "false" }
        ));
        if self.private_devices {
            out.push_str("PrivateDevices=true\n");
        }
        out.push_str(&format!(
            "RestrictAddressFamilies={}\n",
            self.restrict_address_families.join(" ")
        ));

        if let Some(mem) = &self.memory_max {
            out.push_str(&format!("MemoryMax={}\n", mem));
        }
        if let Some(tasks) = self.tasks_max {
            out.push_str(&format!("TasksMax={}\n", tasks));
        }

        out.push_str("\n[Install]\n");
        match self.scope {
            SystemdUnitScope::System => out.push_str("WantedBy=multi-user.target\n"),
            SystemdUnitScope::User => out.push_str("WantedBy=default.target\n"),
        }

        out
    }
}

/// Configuration déclarative d'une unité de socket Systemd (`.socket`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdSocketUnitConfig {
    pub scope: SystemdUnitScope,
    pub socket_name: String,
    pub description: String,
    pub listen_stream: String,
    pub socket_mode: String,
    pub pass_credentials: bool,
    pub remove_on_stop: bool,
}

impl SystemdSocketUnitConfig {
    pub fn default_system(socket_path: &str) -> Self {
        Self {
            scope: SystemdUnitScope::System,
            socket_name: "hakilled".to_string(),
            description: "Hakille Sovereign Cognitive OS IPC Socket".to_string(),
            listen_stream: socket_path.to_string(),
            socket_mode: "0600".to_string(),
            pass_credentials: true,
            remove_on_stop: true,
        }
    }

    pub fn default_user() -> Self {
        Self {
            scope: SystemdUnitScope::User,
            socket_name: "hakilled".to_string(),
            description: "Hakille Sovereign Cognitive OS IPC Socket (User Session)".to_string(),
            listen_stream: "%t/hakille/ipc.sock".to_string(),
            socket_mode: "0600".to_string(),
            pass_credentials: true,
            remove_on_stop: true,
        }
    }

    /// Génère le contenu au format INI pour le fichier `.socket`.
    pub fn render_unit(&self) -> String {
        let mut out = String::new();
        out.push_str("[Unit]\n");
        out.push_str(&format!("Description={}\n", self.description));
        out.push_str(&format!("PartOf={}.service\n\n", self.socket_name));

        out.push_str("[Socket]\n");
        out.push_str(&format!("ListenStream={}\n", self.listen_stream));
        out.push_str(&format!("SocketMode={}\n", self.socket_mode));
        if self.pass_credentials {
            out.push_str("PassCredentials=true\n");
        }
        if self.remove_on_stop {
            out.push_str("RemoveOnStop=true\n");
        }

        out.push_str("\n[Install]\n");
        out.push_str("WantedBy=sockets.target\n");

        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinuxDaemonConfig {
    pub socket_path: String,
    pub uid: u32,
    pub gid: u32,
    pub drop_privileges: bool,
}

pub struct LinuxDaemon {
    config: LinuxDaemonConfig,
    is_running: bool,
    watchdog: Option<SystemdWatchdog>,
}

impl LinuxDaemon {
    pub fn new(socket_path: impl Into<String>) -> Self {
        let watchdog = SystemdWatchdog::from_env();
        Self {
            config: LinuxDaemonConfig {
                socket_path: socket_path.into(),
                uid: 1000,
                gid: 1000,
                drop_privileges: true,
            },
            is_running: false,
            watchdog,
        }
    }

    pub fn start(&mut self) {
        self.is_running = true;
    }

    pub fn stop(&mut self) {
        self.is_running = false;
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn config(&self) -> &LinuxDaemonConfig {
        &self.config
    }

    pub fn watchdog(&self) -> Option<&SystemdWatchdog> {
        self.watchdog.as_ref()
    }

    pub fn notify_ready(&self) -> Result<bool, SystemdError> {
        notify(&SystemdNotifyState::Ready)
    }

    pub fn notify_stopping(&self) -> Result<bool, SystemdError> {
        notify(&SystemdNotifyState::Stopping)
    }

    pub fn notify_watchdog(&self) -> Result<bool, SystemdError> {
        notify(&SystemdNotifyState::Watchdog)
    }

    pub fn notify_status(&self, status: &str) -> Result<bool, SystemdError> {
        notify(&SystemdNotifyState::Status(status))
    }

    pub fn check_socket_activation(
        &self,
        unset_env: bool,
    ) -> Result<Vec<ActivatedSocket>, SystemdError> {
        listen_fds(unset_env)
    }

    pub fn generate_service_unit(&self, scope: SystemdUnitScope, exec_path: &str) -> String {
        match scope {
            SystemdUnitScope::System => {
                SystemdServiceUnitConfig::default_system(exec_path).render_unit()
            }
            SystemdUnitScope::User => {
                SystemdServiceUnitConfig::default_user(exec_path).render_unit()
            }
        }
    }

    pub fn generate_socket_unit(&self, scope: SystemdUnitScope) -> String {
        match scope {
            SystemdUnitScope::System => {
                SystemdSocketUnitConfig::default_system(&self.config.socket_path).render_unit()
            }
            SystemdUnitScope::User => SystemdSocketUnitConfig::default_user().render_unit(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_systemd_notify_state_formatting() {
        assert_eq!(SystemdNotifyState::Ready.as_str(), "READY=1");
        assert_eq!(SystemdNotifyState::Stopping.as_str(), "STOPPING=1");
        assert_eq!(SystemdNotifyState::Watchdog.as_str(), "WATCHDOG=1");
        assert_eq!(
            SystemdNotifyState::WatchdogTrigger.as_str(),
            "WATCHDOG=trigger"
        );
        assert_eq!(
            SystemdNotifyState::Status("Ready and serving").as_str(),
            "STATUS=Ready and serving"
        );
        assert_eq!(SystemdNotifyState::MainPid(4242).as_str(), "MAINPID=4242");
        assert_eq!(SystemdNotifyState::Errno(2).as_str(), "ERRNO=2");
        assert_eq!(SystemdNotifyState::Reloading.as_str(), "RELOADING=1");
    }

    #[test]
    fn test_notify_unbound_noop_when_env_unset() {
        assert!(notify(&SystemdNotifyState::Ready).is_ok());
    }

    #[test]
    fn test_notify_to_socket_roundtrip() {
        use std::os::unix::net::UnixDatagram;
        let temp_dir = std::env::temp_dir();
        let sock_path = temp_dir.join(format!("test_notify_{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&sock_path);

        let receiver = UnixDatagram::bind(&sock_path).expect("bind receiver socket");
        receiver
            .set_nonblocking(true)
            .expect("set receiver nonblocking");

        notify_to_socket(sock_path.to_str().unwrap(), &SystemdNotifyState::Ready)
            .expect("send notification");

        let mut buf = [0u8; 64];
        let (bytes, _) = receiver.recv_from(&mut buf).expect("receive datagram");
        let received = std::str::from_utf8(&buf[..bytes]).unwrap();
        assert_eq!(received, "READY=1\n");

        let _ = std::fs::remove_file(&sock_path);
    }

    #[test]
    fn test_parse_listen_fds_valid() {
        let current_pid = 5000;
        let sockets = parse_listen_fds_from(
            Some("2"),
            Some("5000"),
            Some("hakille-ipc:hakille-metrics"),
            current_pid,
        );
        assert_eq!(sockets.len(), 2);
        assert_eq!(sockets[0].fd, SD_LISTEN_FDS_START);
        assert_eq!(sockets[0].name.as_deref(), Some("hakille-ipc"));
        assert_eq!(sockets[1].fd, SD_LISTEN_FDS_START + 1);
        assert_eq!(sockets[1].name.as_deref(), Some("hakille-metrics"));
    }

    #[test]
    fn test_parse_listen_fds_pid_mismatch() {
        let sockets = parse_listen_fds_from(Some("2"), Some("9999"), None, 5000);
        assert!(
            sockets.is_empty(),
            "Mismatched PID must yield 0 descriptors"
        );
    }

    #[test]
    fn test_parse_listen_fds_invalid_count() {
        assert!(parse_listen_fds_from(Some("0"), None, None, 1).is_empty());
        assert!(parse_listen_fds_from(Some("not_a_num"), None, None, 1).is_empty());
        assert!(parse_listen_fds_from(None, None, None, 1).is_empty());
    }

    #[test]
    fn test_systemd_watchdog_calculation() {
        let wd = SystemdWatchdog::from_duration(std::time::Duration::from_secs(30));
        assert_eq!(wd.interval(), std::time::Duration::from_secs(30));
        assert_eq!(
            wd.recommended_ping_interval(),
            std::time::Duration::from_secs(15)
        );
    }

    #[test]
    fn test_service_unit_generation_system() {
        let config = SystemdServiceUnitConfig::default_system("/usr/local/bin/hakilled");
        let rendered = config.render_unit();
        assert!(rendered.contains("[Unit]"));
        assert!(rendered.contains("Description=Hakille Sovereign Cognitive OS Daemon"));
        assert!(rendered.contains("Type=notify"));
        assert!(rendered.contains("NotifyAccess=main"));
        assert!(rendered.contains("ExecStart=/usr/local/bin/hakilled --serve"));
        assert!(rendered.contains("Restart=on-failure"));
        assert!(rendered.contains("WatchdogSec=30s"));
        assert!(rendered.contains("ProtectSystem=strict"));
        assert!(rendered.contains("PrivateTmp=true"));
        assert!(rendered.contains("PrivateDevices=true"));
        assert!(rendered.contains("WantedBy=multi-user.target"));
    }

    #[test]
    fn test_service_unit_generation_user() {
        let config = SystemdServiceUnitConfig::default_user("/opt/hakille/bin/hakilled");
        let rendered = config.render_unit();
        assert!(rendered.contains("[Unit]"));
        assert!(
            rendered.contains("Description=Hakille Sovereign Cognitive OS Daemon (User Session)")
        );
        assert!(rendered.contains("Type=notify"));
        assert!(rendered.contains("WantedBy=default.target"));
        assert!(!rendered.contains("RuntimeDirectory="));
    }

    #[test]
    fn test_socket_unit_generation() {
        let socket_config = SystemdSocketUnitConfig::default_system("/run/hakille/ipc.sock");
        let rendered = socket_config.render_unit();
        assert!(rendered.contains("[Unit]"));
        assert!(rendered.contains("PartOf=hakilled.service"));
        assert!(rendered.contains("[Socket]"));
        assert!(rendered.contains("ListenStream=/run/hakille/ipc.sock"));
        assert!(rendered.contains("SocketMode=0600"));
        assert!(rendered.contains("PassCredentials=true"));
        assert!(rendered.contains("WantedBy=sockets.target"));

        let user_socket = SystemdSocketUnitConfig::default_user();
        let user_rendered = user_socket.render_unit();
        assert!(user_rendered.contains("ListenStream=%t/hakille/ipc.sock"));
    }
}
