//! Exécution WASM réelle d'agents sandboxés (AEl minimal Hakille).
//!
//! # Conventions AEl minimal (Agent Execution Interface)
//!
//! Un module agent expose exactement :
//! - `memory` : mémoire linéaire exportée ;
//! - `run : (i32, i32) -> (i32, i32)` : `run(input_ptr, input_len)` retourne
//!   `(output_ptr, output_len)`.
//!
//! Flux d'exécution hôte :
//! 1. l'hôte écrit l'input JSON (UTF-8) **à la fin de la mémoire linéaire**
//!    courante, en grandissant la mémoire si nécessaire (borné par
//!    `RunLimits::max_memory_pages`) — l'adresse passée est donc relative à
//!    l'état du guest, pas une constante ;
//! 2. l'hôte appelle `run(input_ptr, input_len)` ;
//! 3. l'hôte lit `output_len` octets à `output_ptr` et exige de l'UTF-8
//!    **et du JSON valide**. La chaîne JSON originale est retournée telle
//!    quelle (pas de re-sérialisation).
//!
//! Aucun import n'est lié (ni WASI, ni `env`) : un module qui importe quoi
//! que ce soit est refusé dès `load`. Aucune horloge, aucun accès
//! fichiers/réseau/env/args : zéro ambient authority. Les seuls I/O sont
//! l'input et l'output ci-dessus.
//!
//! Chaque `run` instancie un `Store` neuf : les agents sont donc
//! déterministes et sans état résiduel entre deux appels.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use wasmtime::{
    Config, Engine, ExternType, Linker, Module, ResourceLimiter, Store, TypedFunc, ValType,
};

/// Intervalle du tic d'epoch partagé par un `AgentSandbox` (timeout à la
/// granularité de ce tic).
const EPOCH_TICK_MS: u64 = 5;

/// Quotas réellement appliqués à chaque `run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    /// Fuel wasmtime consommé ; épuisement → `AgentError::FuelExhausted`.
    pub fuel: u64,
    /// Budget temps ; expiration → `AgentError::Timeout` (epoch interruption).
    pub timeout: Duration,
    /// Mémoire linéaire maximale, en pages de 64 Kio.
    pub max_memory_pages: u32,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            fuel: 50_000_000,
            timeout: Duration::from_secs(5),
            max_memory_pages: 256,
        }
    }
}

/// Erreurs honnêtes du bac à sable WASM.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    #[error("Agent '{agent}' : module WASM/WAT invalide : {message}")]
    InvalidModule { agent: String, message: String },
    #[error("Agent '{agent}' : ABI AEl invalide : {message}")]
    InvalidAbi { agent: String, message: String },
    #[error("Agent '{agent}' : import hôte interdit (aucun import n'est lié) : {message}")]
    ImportNotAllowed { agent: String, message: String },
    #[error("Agent '{agent}' : instanciation refusée : {message}")]
    Instantiation { agent: String, message: String },
    #[error("Agent '{agent}' : fuel épuisé")]
    FuelExhausted { agent: String },
    #[error("Agent '{agent}' : timeout après {timeout:?}")]
    Timeout { agent: String, timeout: Duration },
    #[error("Agent '{agent}' : limite mémoire dépassée (max {max_pages} pages)")]
    MemoryLimitExceeded { agent: String, max_pages: u32 },
    #[error("Agent '{agent}' : entrée trop volumineuse ({input_len} octets, {max_bytes} max)")]
    InputTooLarge {
        agent: String,
        input_len: usize,
        max_bytes: usize,
    },
    #[error("Agent '{agent}' : sortie invalide : {message}")]
    InvalidOutput { agent: String, message: String },
    #[error("Agent '{agent}' : trap WASM : {message}")]
    Trap { agent: String, message: String },
    #[error("Erreur interne du bac à sable WASM : {message}")]
    Internal { message: String },
}

impl From<AgentError> for hakille_core::HakilleError {
    fn from(err: AgentError) -> Self {
        hakille_core::HakilleError::Internal(format!("Agent WASM : {err}"))
    }
}

/// État hôte d'une instance : applique la limite mémoire de `RunLimits` et
/// mémorise tout refus de croissance (pour mapper vers
/// `AgentError::MemoryLimitExceeded` de façon honnête).
struct SandboxState {
    memory_limit_bytes: usize,
    max_memory_pages: u32,
    memory_denied: bool,
}

impl ResourceLimiter for SandboxState {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.memory_limit_bytes {
            self.memory_denied = true;
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(true)
    }

    fn instances(&self) -> usize {
        1
    }

    fn tables(&self) -> usize {
        4
    }

    fn memories(&self) -> usize {
        1
    }
}

/// Tic d'epoch partagé : incrémente l'epoch du moteur toutes les
/// `EPOCH_TICK_MS` ms. Un tic unique partagé synchronise tous les stores sur
/// la même horloge, ce qui évite qu'un timeout n'interrompe l'exécution d'un
/// autre agent (les deadlines sont relatives au même compteur). Le thread
/// s'arrête quand le dernier `Arc` est libéré.
#[derive(Debug)]
struct EpochTicker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl EpochTicker {
    fn start(engine: Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            while !stop_thread.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(EPOCH_TICK_MS));
                engine.increment_epoch();
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Fabrique d'agents WASM sandboxés (moteur wasmtime + tic d'epoch).
#[derive(Clone, Debug)]
pub struct AgentSandbox {
    engine: Engine,
    ticker: Arc<EpochTicker>,
}

/// Agent compilé, prêt à être exécuté. Conserve le tic d'epoch vivant même
/// si le `AgentSandbox` d'origine est libéré.
#[derive(Debug)]
pub struct Agent {
    name: String,
    engine: Engine,
    module: Module,
    _ticker: Arc<EpochTicker>,
}

impl AgentSandbox {
    /// Crée un bac à sable : fuel + epoch interruption activés, aucun import
    /// lié (fail-closed).
    pub fn new() -> Result<Self, AgentError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| AgentError::Internal {
            message: format!("initialisation wasmtime impossible : {e}"),
        })?;
        let ticker = Arc::new(EpochTicker::start(engine.clone()));
        Ok(Self { engine, ticker })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Compile un agent depuis du WAT source **ou** du wasm binaire, et
    /// valide l'ABI AEl (`memory` + `run(i32,i32)->(i32,i32)`) ainsi que
    /// l'absence de tout import.
    pub fn load(&self, name: &str, wat_or_wasm: impl AsRef<[u8]>) -> Result<Agent, AgentError> {
        if name.trim().is_empty() {
            return Err(AgentError::InvalidAbi {
                agent: name.to_string(),
                message: "nom d'agent vide".to_string(),
            });
        }
        let module =
            Module::new(&self.engine, wat_or_wasm).map_err(|e| AgentError::InvalidModule {
                agent: name.to_string(),
                message: e.to_string(),
            })?;
        Self::validate_abi(name, &module)?;
        Ok(Agent {
            name: name.to_string(),
            engine: self.engine.clone(),
            module,
            _ticker: Arc::clone(&self.ticker),
        })
    }

    /// Raccourci : `Agent::run` (mêmes sémantiques).
    pub fn run(
        &self,
        agent: &mut Agent,
        input_json: &str,
        limits: RunLimits,
    ) -> Result<String, AgentError> {
        agent.run(input_json, limits)
    }

    fn validate_abi(name: &str, module: &Module) -> Result<(), AgentError> {
        if let Some(import) = module.imports().next() {
            return Err(AgentError::ImportNotAllowed {
                agent: name.to_string(),
                message: format!("{}::{} ({:?})", import.module(), import.name(), import.ty()),
            });
        }

        let mut has_memory = false;
        let mut has_run = false;
        for export in module.exports() {
            match export.name() {
                "memory" => match export.ty() {
                    ExternType::Memory(_) => has_memory = true,
                    other => {
                        return Err(AgentError::InvalidAbi {
                            agent: name.to_string(),
                            message: format!("export `memory` de type inattendu : {other:?}"),
                        });
                    }
                },
                "run" => match export.ty() {
                    ExternType::Func(func) => {
                        let params: Vec<ValType> = func.params().collect();
                        let results: Vec<ValType> = func.results().collect();
                        if is_i32_pair(&params) && is_i32_pair(&results) {
                            has_run = true;
                        } else {
                            return Err(AgentError::InvalidAbi {
                                agent: name.to_string(),
                                message: format!(
                                    "export `run` doit être (i32,i32)->(i32,i32), vu ({params:?})->({results:?})"
                                ),
                            });
                        }
                    }
                    other => {
                        return Err(AgentError::InvalidAbi {
                            agent: name.to_string(),
                            message: format!("export `run` non-fonction : {other:?}"),
                        });
                    }
                },
                _ => {}
            }
        }

        if !has_memory {
            return Err(AgentError::InvalidAbi {
                agent: name.to_string(),
                message: "export `memory` manquant".to_string(),
            });
        }
        if !has_run {
            return Err(AgentError::InvalidAbi {
                agent: name.to_string(),
                message: "export `run` manquant".to_string(),
            });
        }
        Ok(())
    }
}

impl Agent {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Exécute `run(input_ptr, input_len)` sur un `Store` neuf (déterminisme)
    /// avec fuel, timeout (epoch) et mémoire bornés.
    pub fn run(&mut self, input_json: &str, limits: RunLimits) -> Result<String, AgentError> {
        let memory_limit_bytes =
            usize::try_from(u64::from(limits.max_memory_pages) * 65_536).unwrap_or(usize::MAX);
        let state = SandboxState {
            memory_limit_bytes,
            max_memory_pages: limits.max_memory_pages,
            memory_denied: false,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| state as &mut dyn ResourceLimiter);
        store
            .set_fuel(limits.fuel)
            .map_err(|e| AgentError::Internal {
                message: format!("set_fuel impossible : {e}"),
            })?;
        store.set_epoch_deadline(timeout_ticks(limits.timeout));

        self.execute(&mut store, input_json, &limits)
    }

    fn execute(
        &self,
        store: &mut Store<SandboxState>,
        input_json: &str,
        limits: &RunLimits,
    ) -> Result<String, AgentError> {
        let linker = Linker::<SandboxState>::new(&self.engine);
        let instance_result = linker.instantiate(&mut *store, &self.module);
        let instance = match instance_result {
            Ok(instance) => instance,
            Err(e) => return Err(self.map_instantiate_error(store, &e)),
        };

        let memory =
            instance
                .get_memory(&mut *store, "memory")
                .ok_or_else(|| AgentError::InvalidAbi {
                    agent: self.name.clone(),
                    message: "export `memory` introuvable à l'instanciation".to_string(),
                })?;
        let run_fn: TypedFunc<(i32, i32), (i32, i32)> = instance
            .get_typed_func(&mut *store, "run")
            .map_err(|e| AgentError::InvalidAbi {
                agent: self.name.clone(),
                message: format!("export `run(i32,i32)->(i32,i32)` introuvable : {e}"),
            })?;

        let input = input_json.as_bytes();
        let input_ptr = memory.data_size(&*store);
        let input_end =
            input_ptr
                .checked_add(input.len())
                .ok_or_else(|| AgentError::InputTooLarge {
                    agent: self.name.clone(),
                    input_len: input.len(),
                    max_bytes: 0,
                })?;
        if input_end > usize::try_from(i32::MAX).unwrap_or(usize::MAX)
            || input_end > store.data().memory_limit_bytes
        {
            return Err(AgentError::InputTooLarge {
                agent: self.name.clone(),
                input_len: input.len(),
                max_bytes: store.data().memory_limit_bytes.saturating_sub(input_ptr),
            });
        }
        if input_end > input_ptr {
            let current_pages = memory.size(&*store);
            let needed_pages = u64::try_from(input_end)
                .unwrap_or(u64::MAX)
                .div_ceil(65_536);
            if needed_pages > current_pages {
                let grow_result = memory.grow(&mut *store, needed_pages - current_pages);
                if let Err(e) = grow_result {
                    if store.data().memory_denied {
                        return Err(self.memory_limit_error(store));
                    }
                    return Err(AgentError::InvalidAbi {
                        agent: self.name.clone(),
                        message: format!(
                            "mémoire du guest incapable de grandir pour l'entrée : {e}"
                        ),
                    });
                }
            }
            memory.data_mut(&mut *store)[input_ptr..input_end].copy_from_slice(input);
        }

        let input_ptr = i32::try_from(input_ptr).map_err(|_| AgentError::InputTooLarge {
            agent: self.name.clone(),
            input_len: input.len(),
            max_bytes: store.data().memory_limit_bytes,
        })?;
        let input_len = i32::try_from(input.len()).map_err(|_| AgentError::InputTooLarge {
            agent: self.name.clone(),
            input_len: input.len(),
            max_bytes: store.data().memory_limit_bytes,
        })?;

        let call_result = run_fn.call(&mut *store, (input_ptr, input_len));
        let (output_ptr, output_len) = match call_result {
            Ok(values) => values,
            Err(e) => return Err(self.map_call_error(store, &e, limits)),
        };
        if store.data().memory_denied {
            return Err(self.memory_limit_error(store));
        }

        let size = memory.data_size(&*store);
        let start = usize::try_from(output_ptr).unwrap_or(usize::MAX);
        let len = usize::try_from(output_len).unwrap_or(usize::MAX);
        let end = match start.checked_add(len).filter(|end| *end <= size) {
            Some(end) => end,
            None => {
                return Err(AgentError::InvalidOutput {
                    agent: self.name.clone(),
                    message: format!(
                        "plage de sortie ({output_ptr}, {output_len}) hors mémoire ({size} octets)"
                    ),
                });
            }
        };
        let bytes = memory.data(&*store)[start..end].to_vec();
        let output = String::from_utf8(bytes).map_err(|e| AgentError::InvalidOutput {
            agent: self.name.clone(),
            message: format!("sortie non UTF-8 : {e}"),
        })?;
        serde_json::from_str::<serde_json::Value>(&output).map_err(|e| {
            let preview: String = output.chars().take(200).collect();
            AgentError::InvalidOutput {
                agent: self.name.clone(),
                message: format!("sortie non JSON ({e}) : {preview:?}"),
            }
        })?;
        Ok(output)
    }

    fn memory_limit_error(&self, store: &Store<SandboxState>) -> AgentError {
        AgentError::MemoryLimitExceeded {
            agent: self.name.clone(),
            max_pages: store.data().max_memory_pages,
        }
    }

    fn map_instantiate_error(
        &self,
        store: &Store<SandboxState>,
        e: &wasmtime::Error,
    ) -> AgentError {
        if store.data().memory_denied {
            return self.memory_limit_error(store);
        }
        let message = e.to_string();
        if message.contains("unknown import") || message.contains("incompatible import type") {
            return AgentError::ImportNotAllowed {
                agent: self.name.clone(),
                message,
            };
        }
        AgentError::Instantiation {
            agent: self.name.clone(),
            message,
        }
    }

    fn map_call_error(
        &self,
        store: &Store<SandboxState>,
        e: &wasmtime::Error,
        limits: &RunLimits,
    ) -> AgentError {
        if let Some(trap) = e.downcast_ref::<wasmtime::Trap>() {
            match trap {
                wasmtime::Trap::OutOfFuel => {
                    return AgentError::FuelExhausted {
                        agent: self.name.clone(),
                    };
                }
                wasmtime::Trap::Interrupt => {
                    return AgentError::Timeout {
                        agent: self.name.clone(),
                        timeout: limits.timeout,
                    };
                }
                _ => {}
            }
        }
        if store.data().memory_denied {
            return self.memory_limit_error(store);
        }
        AgentError::Trap {
            agent: self.name.clone(),
            message: e.to_string(),
        }
    }
}

/// `[i32, i32]` exactement (ValType n'implémente pas `PartialEq`).
fn is_i32_pair(types: &[ValType]) -> bool {
    types.len() == 2 && types.iter().all(|t| t.matches(&ValType::I32))
}

/// Nombre de tics d'epoch avant déclenchement du timeout (0 = immédiat).
fn timeout_ticks(timeout: Duration) -> u64 {
    if timeout.is_zero() {
        return 0;
    }
    let tick_nanos = u128::from(EPOCH_TICK_MS) * 1_000_000;
    let ticks = timeout.as_nanos().div_ceil(tick_nanos);
    u64::try_from(ticks).unwrap_or(u64::MAX).max(1)
}

/// Agent de référence `echo-agent` : retourne l'input tel quel.
pub const ECHO_AGENT_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (func (export "run") (param i32 i32) (result i32 i32)
    (local.get 0)
    (local.get 1)))"#;

/// Agent de référence `count-agent` : retourne `{"count":N}` où `N` est le
/// nombre d'octets UTF-8 de l'input. Zone de sortie fixe en basse mémoire
/// (adresse 16).
pub const COUNT_AGENT_WAT: &str = r#"(module
  (memory (export "memory") 1)

  (func $digit (param $addr i32) (param $d i32)
    (i32.store8 (local.get $addr)
      (i32.add (local.get $d) (i32.const 48))))

  (func $itoa (param $value i32) (param $dst i32) (result i32)
    (local $n i32)
    (local $v i32)
    (local $i i32)
    (local $j i32)
    (local $t i32)
    (local.set $v (local.get $value))
    (if (i32.eqz (local.get $v))
      (then
        (call $digit (local.get $dst) (i32.const 0))
        (return (i32.const 1))))
    (block $done
      (loop $digits
        (br_if $done (i32.eqz (local.get $v)))
        (call $digit
          (i32.add (local.get $dst) (local.get $n))
          (i32.rem_u (local.get $v) (i32.const 10)))
        (local.set $v (i32.div_u (local.get $v) (i32.const 10)))
        (local.set $n (i32.add (local.get $n) (i32.const 1)))
        (br $digits)))
    (local.set $i (i32.const 0))
    (local.set $j (i32.sub (local.get $n) (i32.const 1)))
    (block $rev_done
      (loop $rev
        (br_if $rev_done (i32.ge_u (local.get $i) (local.get $j)))
        (local.set $t (i32.load8_u (i32.add (local.get $dst) (local.get $i))))
        (i32.store8
          (i32.add (local.get $dst) (local.get $i))
          (i32.load8_u (i32.add (local.get $dst) (local.get $j))))
        (i32.store8 (i32.add (local.get $dst) (local.get $j)) (local.get $t))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (local.set $j (i32.sub (local.get $j) (i32.const 1)))
        (br $rev)))
    (local.get $n))

  (func (export "run") (param $in_ptr i32) (param $in_len i32) (result i32 i32)
    (local $n i32)
    (i32.store8 (i32.const 16) (i32.const 123))
    (i32.store8 (i32.const 17) (i32.const 34))
    (i32.store8 (i32.const 18) (i32.const 99))
    (i32.store8 (i32.const 19) (i32.const 111))
    (i32.store8 (i32.const 20) (i32.const 117))
    (i32.store8 (i32.const 21) (i32.const 110))
    (i32.store8 (i32.const 22) (i32.const 116))
    (i32.store8 (i32.const 23) (i32.const 34))
    (i32.store8 (i32.const 24) (i32.const 58))
    (local.set $n (call $itoa (local.get $in_len) (i32.const 25)))
    (i32.store8 (i32.add (i32.const 25) (local.get $n)) (i32.const 125))
    (i32.const 16)
    (i32.add (i32.const 10) (local.get $n))))"#;

/// Agents de référence embarqués (nom, WAT source) : compilables par
/// `AgentSandbox::load` sans aucun fichier externe.
pub fn reference_agents() -> Vec<(&'static str, &'static str)> {
    vec![
        ("echo-agent", ECHO_AGENT_WAT),
        ("count-agent", COUNT_AGENT_WAT),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox() -> AgentSandbox {
        AgentSandbox::new().expect("bac à sable wasmtime")
    }

    #[test]
    fn reference_agents_load_and_run() {
        let sandbox = sandbox();
        let input = r#"{"msg":"bonjour","n":1}"#;
        for (name, wat) in reference_agents() {
            let mut agent = sandbox.load(name, wat).expect("load WAT de référence");
            assert_eq!(agent.name(), name);
            let output = agent.run(input, RunLimits::default()).expect("run");
            if name == "echo-agent" {
                assert_eq!(output, input);
            } else {
                assert_eq!(output, format!(r#"{{"count":{}}}"#, input.len()));
            }
        }
    }

    #[test]
    fn load_accepts_wat_source_and_compiled_wasm() {
        let sandbox = sandbox();
        let input = r#"{"a":"é"}"#;

        let mut from_wat = sandbox.load("echo-wat", ECHO_AGENT_WAT).unwrap();
        assert_eq!(
            from_wat.run(input, RunLimits::default()).unwrap(),
            input,
            "WAT source"
        );

        let wasm = wat::parse_str(ECHO_AGENT_WAT).expect("compilation WAT→wasm");
        let mut from_wasm = sandbox.load("echo-wasm", &wasm).unwrap();
        assert_eq!(
            from_wasm.run(input, RunLimits::default()).unwrap(),
            input,
            "wasm binaire"
        );
    }

    #[test]
    fn count_agent_counts_utf8_bytes() {
        let sandbox = sandbox();
        let mut agent = sandbox.load("count-agent", COUNT_AGENT_WAT).unwrap();
        let input = r#"{"texte":"héllo wörld"}"#;
        assert!(input.len() > input.chars().count());
        let output = agent.run(input, RunLimits::default()).unwrap();
        assert_eq!(output, format!(r#"{{"count":{}}}"#, input.len()));
    }

    #[test]
    fn runs_are_deterministic() {
        let sandbox = sandbox();
        let input = r#"{"question":"réponse","n":42}"#;

        let mut a = sandbox.load("echo-a", ECHO_AGENT_WAT).unwrap();
        let first = a.run(input, RunLimits::default()).unwrap();
        let second = a.run(input, RunLimits::default()).unwrap();
        assert_eq!(first, second, "même agent, même input → même output");

        let mut b = sandbox.load("echo-b", ECHO_AGENT_WAT).unwrap();
        assert_eq!(
            b.run(input, RunLimits::default()).unwrap(),
            first,
            "instance neuve → même output"
        );

        let mut c = sandbox.load("count-c", COUNT_AGENT_WAT).unwrap();
        let count_1 = c.run(input, RunLimits::default()).unwrap();
        let count_2 = c.run(input, RunLimits::default()).unwrap();
        assert_eq!(count_1, count_2);
    }

    #[test]
    fn fuel_exhaustion_is_reported() {
        let sandbox = sandbox();
        let loop_wat = r#"(module
            (memory (export "memory") 1)
            (func (export "run") (param i32 i32) (result i32 i32)
              (loop (br 0))
              (i32.const 0)
              (i32.const 0)))"#;
        let mut agent = sandbox.load("loop-agent", loop_wat).unwrap();
        let limits = RunLimits {
            fuel: 100_000,
            timeout: Duration::from_secs(30),
            max_memory_pages: 2,
        };
        let err = agent.run(r#"{"go":true}"#, limits).unwrap_err();
        assert!(
            matches!(err, AgentError::FuelExhausted { .. }),
            "attendu FuelExhausted, vu {err:?}"
        );
    }

    #[test]
    fn timeout_is_reported() {
        let sandbox = sandbox();
        let loop_wat = r#"(module
            (memory (export "memory") 1)
            (func (export "run") (param i32 i32) (result i32 i32)
              (loop (br 0))
              (i32.const 0)
              (i32.const 0)))"#;
        let mut agent = sandbox.load("hang-agent", loop_wat).unwrap();
        let limits = RunLimits {
            fuel: 1_000_000_000,
            timeout: Duration::from_millis(100),
            max_memory_pages: 2,
        };
        let err = agent.run(r#"{"go":true}"#, limits).unwrap_err();
        assert!(
            matches!(err, AgentError::Timeout { .. }),
            "attendu Timeout, vu {err:?}"
        );
    }

    #[test]
    fn memory_limit_is_enforced() {
        let sandbox = sandbox();
        let greedy_wat = r#"(module
            (memory (export "memory") 1)
            (func (export "run") (param i32 i32) (result i32 i32)
              (drop (memory.grow (i32.const 4)))
              (local.get 0)
              (local.get 1)))"#;
        let mut agent = sandbox.load("greedy-agent", greedy_wat).unwrap();
        let limits = RunLimits {
            fuel: 50_000_000,
            timeout: Duration::from_secs(5),
            max_memory_pages: 2,
        };
        let err = agent.run(r#"{"go":true}"#, limits).unwrap_err();
        assert!(
            matches!(err, AgentError::MemoryLimitExceeded { .. }),
            "attendu MemoryLimitExceeded, vu {err:?}"
        );
    }

    #[test]
    fn static_memory_above_limit_is_rejected() {
        let sandbox = sandbox();
        let fat_memory = r#"(module
            (memory (export "memory") 4)
            (func (export "run") (param i32 i32) (result i32 i32)
              (i32.const 0)
              (i32.const 0)))"#;
        let mut agent = sandbox.load("fat-memory", fat_memory).unwrap();
        let limits = RunLimits {
            fuel: 50_000_000,
            timeout: Duration::from_secs(5),
            max_memory_pages: 1,
        };
        let err = agent.run(r#"{}"#, limits).unwrap_err();
        assert!(
            matches!(err, AgentError::MemoryLimitExceeded { .. }),
            "attendu MemoryLimitExceeded, vu {err:?}"
        );
    }

    #[test]
    fn oversized_input_is_rejected() {
        let sandbox = sandbox();
        let mut agent = sandbox.load("echo-agent", ECHO_AGENT_WAT).unwrap();
        let limits = RunLimits {
            fuel: 50_000_000,
            timeout: Duration::from_secs(5),
            max_memory_pages: 1,
        };
        let big_input = format!(r#"{{"x":"{}"}}"#, "a".repeat(70_000));
        let err = agent.run(&big_input, limits).unwrap_err();
        assert!(
            matches!(err, AgentError::InputTooLarge { .. }),
            "attendu InputTooLarge, vu {err:?}"
        );
    }

    #[test]
    fn invalid_wasm_is_rejected() {
        let sandbox = sandbox();
        let err = sandbox
            .load("garbage", b"not a wasm module at all")
            .unwrap_err();
        assert!(
            matches!(err, AgentError::InvalidModule { .. }),
            "octets aléatoires : {err:?}"
        );

        let err = sandbox
            .load("broken-wat", "(module (func (export")
            .unwrap_err();
        assert!(
            matches!(err, AgentError::InvalidModule { .. }),
            "WAT invalide : {err:?}"
        );

        let err = sandbox.load("empty", b"").unwrap_err();
        assert!(matches!(err, AgentError::InvalidModule { .. }));
    }

    #[test]
    fn invalid_abi_is_rejected_at_load() {
        let sandbox = sandbox();

        let no_memory = r#"(module
            (func (export "run") (param i32 i32) (result i32 i32)
              (local.get 0) (local.get 1)))"#;
        assert!(matches!(
            sandbox.load("no-memory", no_memory).unwrap_err(),
            AgentError::InvalidAbi { .. }
        ));

        let no_run = r#"(module (memory (export "memory") 1))"#;
        assert!(matches!(
            sandbox.load("no-run", no_run).unwrap_err(),
            AgentError::InvalidAbi { .. }
        ));

        let bad_signature = r#"(module
            (memory (export "memory") 1)
            (func (export "run") (param i32) (result i32) (local.get 0)))"#;
        assert!(matches!(
            sandbox.load("bad-signature", bad_signature).unwrap_err(),
            AgentError::InvalidAbi { .. }
        ));
    }

    #[test]
    fn imports_are_refused_fail_closed() {
        let sandbox = sandbox();
        let importing = r#"(module
            (import "env" "ambient" (func $ambient))
            (memory (export "memory") 1)
            (func (export "run") (param i32 i32) (result i32 i32)
              (local.get 0) (local.get 1)))"#;
        let err = sandbox.load("importer", importing).unwrap_err();
        assert!(
            matches!(err, AgentError::ImportNotAllowed { .. }),
            "attendu ImportNotAllowed, vu {err:?}"
        );
    }

    #[test]
    fn non_json_output_is_rejected() {
        let sandbox = sandbox();
        let bad_output = r#"(module
            (memory (export "memory") 1)
            (data (i32.const 0) "pas du json")
            (func (export "run") (param i32 i32) (result i32 i32)
              (i32.const 0)
              (i32.const 11)))"#;
        let mut agent = sandbox.load("bad-output", bad_output).unwrap();
        let err = agent
            .run(r#"{"go":true}"#, RunLimits::default())
            .unwrap_err();
        assert!(
            matches!(err, AgentError::InvalidOutput { .. }),
            "attendu InvalidOutput, vu {err:?}"
        );
    }

    #[test]
    fn out_of_bounds_output_is_rejected() {
        let sandbox = sandbox();
        let oob_output = r#"(module
            (memory (export "memory") 1)
            (func (export "run") (param i32 i32) (result i32 i32)
              (i32.const 65530)
              (i32.const 100)))"#;
        let mut agent = sandbox.load("oob-output", oob_output).unwrap();
        let err = agent
            .run(r#"{"go":true}"#, RunLimits::default())
            .unwrap_err();
        assert!(
            matches!(err, AgentError::InvalidOutput { .. }),
            "attendu InvalidOutput, vu {err:?}"
        );
    }

    #[test]
    fn zero_timeout_traps_immediately_on_loop() {
        let sandbox = sandbox();
        let loop_wat = r#"(module
            (memory (export "memory") 1)
            (func (export "run") (param i32 i32) (result i32 i32)
              (loop (br 0))
              (i32.const 0)
              (i32.const 0)))"#;
        let mut agent = sandbox.load("zero-timeout", loop_wat).unwrap();
        let limits = RunLimits {
            fuel: 1_000_000_000,
            timeout: Duration::ZERO,
            max_memory_pages: 2,
        };
        let err = agent.run(r#"{}"#, limits).unwrap_err();
        assert!(
            matches!(err, AgentError::Timeout { .. }),
            "attendu Timeout, vu {err:?}"
        );
    }
}
