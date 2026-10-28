//! Reproduce the *host* Wasmtime compilation settings, not only CLI defaults.
//! The production Rust core constructs an Engine with fuel metering enabled
//! for selected parser paths. Instrumentation changes generated native code,
//! so successful `wasmtime compile` without fuel is not sufficient evidence.
use std::error::Error;
use wasmtime::{component::Component, Config, Engine};

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args().nth(1).ok_or("usage: host-probe <component.wasm>")?;
    let bytes = std::fs::read(&path)?;
    // Check both modes to distinguish CLI/default success from host failure.
    for metered in [false, true] {
        eprintln!("Windows ARM64 Wasmtime host: compiling {} ({} bytes), fuel={metered}", path, bytes.len());
        let mut config = Config::new();
        config.consume_fuel(metered);
        config.wasm_memory64(false);
        let engine = Engine::new(&config)?;
        let _component = Component::new(&engine, bytes.as_slice())?;
        eprintln!("Windows ARM64 Wasmtime host: compiled fuel={metered}");
    }
    Ok(())
}
