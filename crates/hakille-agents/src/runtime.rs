use crate::capability::{AgentQuotaTracker, CapabilityQuota};
use crate::sandbox::SandboxLimits;
use hakille_core::{HakilleError, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::{FsPerms, WasiCtxBuilder};

/// État hôte d'une instance WASM : contexte WASI preview1 + limites mémoire.
///
/// `WasiP1Ctx` porte le `WasiCtx` (préouvertures, horloges, réseau) et
/// `StoreLimits` borne mémoire/instances/tables (voir `execute_wasm`).
pub struct AgentStoreState {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
    /// Répertoire temp sandboxé pré-ouvert (conservé vivant le temps de
    /// l'exécution, nettoyé best-effort après). `None` si `!allow_fs`.
    _sandbox_dir: Option<PathBuf>,
}

static SANDBOX_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct WasmAgentRuntime {
    engine: Engine,
    quota_tracker: std::sync::Mutex<AgentQuotaTracker>,
}

impl WasmAgentRuntime {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        // wasmtime 48 : l'async est toujours actif (`async_support` retiré).
        // Préemption coopérative : `store.set_epoch_deadline` + incrément
        // depuis une tâche Tokio après `max_execution_time_ms` (en plus du
        // timeout Tokio et du fuel, défense en profondeur).
        config.epoch_interruption(true);

        let engine = Engine::new(&config).map_err(|e| {
            HakilleError::Internal(format!("Failed to init wasmtime engine: {}", e))
        })?;

        Ok(Self {
            engine,
            quota_tracker: std::sync::Mutex::new(
                AgentQuotaTracker::new(CapabilityQuota::default()),
            ),
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
    ///   temp dédié `…/hakille-wasm-sandbox-<pid>-<ctr>` est créé et pré-ouvert
    ///   comme `/sandbox` (`DirPerms::all/read` isolé, nettoyé après exécution).
    /// - Réseau : TCP/UDP/`ip-name-lookup` coupés par défaut ; si
    ///   `allow_network`, TCP + lookup activés MAIS chaque adresse passe par
    ///   `socket_addr_check` contre `egress_allowlist` (vide = tout refusé,
    ///   fail-closed). UDP reste toujours refusé.
    /// - Horloges monotones/wall hôtes, stdio/stderr fermés (pas d'héritage),
    ///   pas d'env/args hérités.
    fn build_wasi_ctx(limits: &SandboxLimits) -> Result<(WasiP1Ctx, Option<PathBuf>)> {
        let mut builder = WasiCtxBuilder::new();

        // Réseau : deny-by-default, allowlist exigée en plus de allow_network.
        if limits.allow_network {
            builder.allow_tcp(true);
            builder.allow_udp(false);
            builder.allow_ip_name_lookup(true);
            let allowlist = limits.egress_allowlist.clone();
            builder.socket_addr_check(move |addr, _use| {
                let allowlist = allowlist.clone();
                Box::pin(async move {
                    let ip = addr.ip().to_string().to_lowercase();
                    allowlist.iter().any(|a| a.trim().to_lowercase() == ip)
                })
            });
        } else {
            builder.allow_tcp(false);
            builder.allow_udp(false);
            builder.allow_ip_name_lookup(false);
            builder.socket_addr_check(|_, _| Box::pin(async { false }));
        }

        // FS : préouverture temp sandboxée seulement si allow_filesystem.
        let mut sandbox_dir: Option<PathBuf> = None;
        if limits.allow_filesystem {
            let ctr = SANDBOX_COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "hakille-wasm-sandbox-{}-{}",
                std::process::id(),
                ctr
            ));
            std::fs::create_dir_all(&dir).map_err(|e| {
                HakilleError::Internal(format!("Failed to create WASI sandbox dir: {e}"))
            })?;
            builder
                .preopened_dir(&dir, "/sandbox", FsPerms::ReadWrite)
                .map_err(|e| {
                    HakilleError::Internal(format!("Failed to preopen WASI sandbox dir: {e}"))
                })?;
            sandbox_dir = Some(dir);
        }

        Ok((builder.build_p1(), sandbox_dir))
    }

    pub async fn execute_wasm(&self, wasm_bytes: &[u8], limits: &SandboxLimits) -> Result<i32> {
        // 0. Quota d'invocations (§31c) : refus fail-closed + AutoRevoke gérée
        //    dans le tracker (tout dépassement révoque et bloque la suite).
        {
            let mut tracker = self
                .quota_tracker
                .lock()
                .map_err(|e| HakilleError::Internal(format!("Quota tracker poisoned: {e}")))?;
            tracker.record_invocation(wasm_bytes.len())?;
        }

        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| HakilleError::Internal(format!("Invalid WASM module: {}", e)))?;

        // 1. Linker WASI strict (`wasi_snapshot_preview1` uniquement) :
        //    - `p1::add_to_linker_async` lie fd_write/path_open/… sur le
        //      `WasiP1Ctx` sandboxé ci-dessus ;
        //    - tout import hors `wasi_snapshot_preview1` (ex. `env`, sockets
        //      bruts) échoue à l'instanciation (fail-closed, aucun import lié).
        let mut linker = Linker::<AgentStoreState>::new(&self.engine);
        p1::add_to_linker_async(&mut linker, |s| &mut s.wasi)
            .map_err(|e| HakilleError::Internal(format!("Failed to link WASI preview1: {e}")))?;

        // 2. Limites mémoire vérifiées : StoreLimits borne la mémoire vive,
        //    le nombre d'instances/tables/mémoires ; le fuel borne les
        //    instructions ; le timeout borne le temps mural ; l'epoch
        //    interrompt les boucles serrées ne cédant jamais la main.
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
                _sandbox_dir: sandbox_dir.clone(),
            },
        );
        store.limiter(|s| &mut s.limits);
        store
            .set_fuel(limits.max_fuel)
            .map_err(|e| HakilleError::Internal(format!("Failed to set fuel: {}", e)))?;
        // Deadline epoch : 1 tick suffit — la tâche ci-dessous incrémente
        // l'epoch globale après `max_execution_time_ms` (piège les boucles
        // infinies même sans yield, en plus du fuel et du timeout Tokio).
        store.set_epoch_deadline(1);

        let timeout_duration = std::time::Duration::from_millis(limits.max_execution_time_ms);
        let epoch_engine = self.engine.clone();
        let epoch_task = tokio::spawn(async move {
            tokio::time::sleep(timeout_duration).await;
            epoch_engine.increment_epoch();
        });

        // Instantiate module through the locked-down linker (WASI preview1 only).
        let instance = linker
            .instantiate_async(&mut store, &module)
            .await
            .map_err(|e| {
                epoch_task.abort();
                Self::cleanup_sandbox_dir(&sandbox_dir);
                HakilleError::Internal(format!("Failed to instantiate WASM: {}", e))
            })?;

        // Look for default run entrypoint if present with timeout enforcement.
        // `run() -> i32` (convention Hakille) prioritaire, sinon `_start`
        // WASI-command (proc_exit(i32) mappé vers Ok(code)).
        let result = if let Ok(run_fn) = instance.get_typed_func::<(), i32>(&mut store, "run") {
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
        };

        epoch_task.abort();
        Self::cleanup_sandbox_dir(&sandbox_dir);
        result
    }

    /// Mappe une erreur d'exécution : `proc_exit(code)` → `Ok(code)` géré par
    /// l'appelant, sinon message fail-closed (fuel épuisé, epoch, mémoire…).
    fn map_exec_error(e: &wasmtime::Error) -> HakilleError {
        if let Some(exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
            return HakilleError::Internal(format!("WASI proc_exit({})", exit.0));
        }
        HakilleError::Internal(format!("WASM execution error: {e}"))
    }

    /// Nettoyage best-effort du répertoire temp sandboxé (jamais d'erreur).
    fn cleanup_sandbox_dir(dir: &Option<PathBuf>) {
        if let Some(d) = dir {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}
