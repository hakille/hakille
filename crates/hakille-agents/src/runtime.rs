use crate::capability::{AgentQuotaTracker, CapabilityQuota};
use crate::sandbox::SandboxLimits;
use hakille_core::{HakilleError, Result};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::{FsPerms, WasiCtxBuilder};

const RUNTIME_EPOCH_TICK_MS: u64 = 5;

/// Répertoire temporaire exclusif RAII (0700 sur Unix, supprimé au `Drop`).
struct SandboxTempDir(PathBuf);

impl SandboxTempDir {
    fn create() -> Result<Self> {
        let base = std::env::temp_dir();
        for _ in 0..8 {
            let dir = base.join(format!("hakille-wasm-sandbox-{}", uuid::Uuid::new_v4()));
            #[cfg(unix)]
            let created = {
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new().mode(0o700).create(&dir)
            };
            #[cfg(not(unix))]
            let created = std::fs::DirBuilder::new().create(&dir);
            match created {
                Ok(()) => return Ok(Self(dir)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(HakilleError::Internal(format!(
                        "Failed to create exclusive WASI sandbox dir: {e}"
                    )));
                }
            }
        }
        Err(HakilleError::Internal(
            "Failed to allocate unique WASI sandbox dir".into(),
        ))
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for SandboxTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => IpAddr::V4(v4),
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
    }
}

/// État hôte d'une instance WASM : contexte WASI preview1 + limites mémoire.
pub struct AgentStoreState {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
    _sandbox_dir: Option<SandboxTempDir>,
}

struct RuntimeEpochTicker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl RuntimeEpochTicker {
    fn spawn(engine: Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("hakille-wasm-runtime-epoch".into())
            .spawn(move || {
                while !stop_flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(RUNTIME_EPOCH_TICK_MS));
                    engine.increment_epoch();
                }
            })
            .ok();
        Self { stop, handle }
    }
}

impl Drop for RuntimeEpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub struct WasmAgentRuntime {
    engine: Engine,
    quota_tracker: std::sync::Mutex<AgentQuotaTracker>,
    _ticker: Arc<RuntimeEpochTicker>,
}

impl WasmAgentRuntime {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);

        let engine = Engine::new(&config).map_err(|e| {
            HakilleError::Internal(format!("Failed to init wasmtime engine: {}", e))
        })?;
        let ticker = Arc::new(RuntimeEpochTicker::spawn(engine.clone()));

        Ok(Self {
            engine,
            quota_tracker: std::sync::Mutex::new(
                AgentQuotaTracker::new(CapabilityQuota::default()),
            ),
            _ticker: ticker,
        })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Remplace le quota appliqué à chaque `execute_wasm` (tests, politiques).
    pub fn set_quota(&self, quota: CapabilityQuota) {
        if let Ok(mut tracker) = self.quota_tracker.lock() {
            *tracker = AgentQuotaTracker::new(quota);
        }
    }

    pub fn quota_revoked(&self) -> bool {
        self.quota_tracker
            .lock()
            .map(|t| t.is_revoked())
            .unwrap_or(false)
    }

    pub fn quota_total_calls(&self) -> u32 {
        self.quota_tracker
            .lock()
            .map(|t| t.total_calls())
            .unwrap_or(0)
    }

    /// Construit un `WasiP1Ctx` sandboxé selon `limits` (wasmtime-wasi 48,
    /// `wasi_snapshot_preview1`) :
    /// - FS : aucun préouvert par défaut ; si `allow_filesystem`, un répertoire
    ///   temporaire exclusif (`UUID v4`, `0700` Unix, non prédictible) est créé
    ///   et pré-ouvert comme `/sandbox` (nettoyé automatiquement par RAII au Drop).
    /// - Réseau : TCP/UDP/`ip-name-lookup` coupés par défaut ; même si
    ///   `allow_network` est actif, `allow_ip_name_lookup(false)` reste coupé
    ///   (anti-exfiltration DNS) et chaque adresse IP littérale normalisée
    ///   (IPv4-mapped compris) doit figurer dans `egress_allowlist`.
    fn build_wasi_ctx(limits: &SandboxLimits) -> Result<(WasiP1Ctx, Option<SandboxTempDir>)> {
        let mut builder = WasiCtxBuilder::new();

        if limits.allow_network {
            let allowed_ips: Vec<IpAddr> = limits
                .egress_allowlist
                .iter()
                .filter_map(|raw| raw.trim().parse::<IpAddr>().ok().map(normalize_ip))
                .collect();
            builder.allow_tcp(!allowed_ips.is_empty());
            builder.allow_udp(false);
            // Toujours désactivé : empêche toute exfiltration via requêtes DNS.
            builder.allow_ip_name_lookup(false);
            builder.socket_addr_check(move |addr, _use| {
                let allowed_ips = allowed_ips.clone();
                Box::pin(async move {
                    let target = normalize_ip(addr.ip());
                    allowed_ips.contains(&target)
                })
            });
        } else {
            builder.allow_tcp(false);
            builder.allow_udp(false);
            builder.allow_ip_name_lookup(false);
            builder.socket_addr_check(|_, _| Box::pin(async { false }));
        }

        let mut sandbox_dir: Option<SandboxTempDir> = None;
        if limits.allow_filesystem {
            let dir = SandboxTempDir::create()?;
            builder
                .preopened_dir(dir.path(), "/sandbox", FsPerms::ReadWrite)
                .map_err(|e| {
                    HakilleError::Internal(format!("Failed to preopen WASI sandbox dir: {e}"))
                })?;
            sandbox_dir = Some(dir);
        }

        Ok((builder.build_p1(), sandbox_dir))
    }

    pub async fn execute_wasm(&self, wasm_bytes: &[u8], limits: &SandboxLimits) -> Result<i32> {
        {
            let mut tracker = self
                .quota_tracker
                .lock()
                .map_err(|e| HakilleError::Internal(format!("Quota tracker poisoned: {e}")))?;
            tracker.record_invocation(wasm_bytes.len())?;
        }

        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| HakilleError::Internal(format!("Invalid WASM module: {}", e)))?;

        let mut linker = Linker::<AgentStoreState>::new(&self.engine);
        p1::add_to_linker_async(&mut linker, |s| &mut s.wasi)
            .map_err(|e| HakilleError::Internal(format!("Failed to link WASI preview1: {e}")))?;

        let (wasi_ctx, sandbox_dir) = Self::build_wasi_ctx(limits)?;
        let store_limits = StoreLimitsBuilder::new()
            .memory_size(limits.max_memory_bytes)
            .instances(1)
            .tables(5)
            .memories(1)
            .trap_on_grow_failure(true)
            .build();

        let mut store = Store::new(
            &self.engine,
            AgentStoreState {
                wasi: wasi_ctx,
                limits: store_limits,
                _sandbox_dir: sandbox_dir,
            },
        );
        store.limiter(|s| &mut s.limits);
        store
            .set_fuel(limits.max_fuel)
            .map_err(|e| HakilleError::Internal(format!("Failed to set fuel: {}", e)))?;
        let epoch_ticks = (limits.max_execution_time_ms / RUNTIME_EPOCH_TICK_MS).max(1);
        store.set_epoch_deadline(epoch_ticks);

        let timeout_duration = Duration::from_millis(limits.max_execution_time_ms);

        let instance = linker
            .instantiate_async(&mut store, &module)
            .await
            .map_err(|e| HakilleError::Internal(format!("Failed to instantiate WASM: {}", e)))?;

        if let Ok(run_fn) = instance.get_typed_func::<(), i32>(&mut store, "run") {
            let execution_future = run_fn.call_async(&mut store, ());
            let res = tokio::time::timeout(timeout_duration, execution_future)
                .await
                .map_err(|_| {
                    HakilleError::Internal(format!(
                        "WASM agent execution timed out after {}ms",
                        limits.max_execution_time_ms
                    ))
                })?
                .map_err(|e| Self::map_exec_error(&e))?;
            Ok(res)
        } else if let Ok(start_fn) = instance.get_typed_func::<(), ()>(&mut store, "_start") {
            let execution_future = start_fn.call_async(&mut store, ());
            let res = tokio::time::timeout(timeout_duration, execution_future)
                .await
                .map_err(|_| {
                    HakilleError::Internal(format!(
                        "WASM agent execution timed out after {}ms",
                        limits.max_execution_time_ms
                    ))
                })?;
            match res {
                Ok(()) => Ok(0),
                Err(e) => {
                    if let Some(exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                        Ok(exit.0)
                    } else {
                        Err(Self::map_exec_error(&e))
                    }
                }
            }
        } else {
            Ok(0)
        }
    }

    fn map_exec_error(e: &wasmtime::Error) -> HakilleError {
        if let Some(exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
            return HakilleError::Internal(format!("WASI proc_exit({})", exit.0));
        }
        HakilleError::Internal(format!("WASM execution error: {e}"))
    }
}
