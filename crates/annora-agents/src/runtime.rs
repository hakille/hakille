use crate::sandbox::SandboxLimits;
use annora_core::{AnnoraError, Result};
use wasmtime::{Config, Engine, Module, Store};

pub struct WasmAgentRuntime {
    engine: Engine,
}

impl WasmAgentRuntime {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.async_support(true);

        let engine = Engine::new(&config)
            .map_err(|e| AnnoraError::Internal(format!("Failed to init wasmtime engine: {}", e)))?;

        Ok(Self { engine })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub async fn execute_wasm(
        &self,
        wasm_bytes: &[u8],
        limits: &SandboxLimits,
    ) -> Result<i32> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| AnnoraError::Internal(format!("Invalid WASM module: {}", e)))?;

        let mut store = Store::new(&self.engine, ());
        store
            .set_fuel(limits.max_fuel)
            .map_err(|e| AnnoraError::Internal(format!("Failed to set fuel: {}", e)))?;

        // Instantiate module
        let instance = wasmtime::Instance::new_async(&mut store, &module, &[]).await
            .map_err(|e| AnnoraError::Internal(format!("Failed to instantiate WASM: {}", e)))?;

        // Look for default run entrypoint if present
        if let Some(run_fn) = instance.get_typed_func::<(), i32>(&mut store, "run").ok() {
            let res = run_fn.call_async(&mut store, ()).await
                .map_err(|e| AnnoraError::Internal(format!("WASM execution error: {}", e)))?;
            Ok(res)
        } else {
            Ok(0)
        }
    }
}
