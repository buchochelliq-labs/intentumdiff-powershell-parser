//! Compile exact staged Wasm with the real Wasmtime 47 Rust host options.
//! Run each config in a *separate process*: Cranelift's oversized-function
//! failure panics, so a single process cannot report later configurations.
use std::error::Error;
use wasmtime::{component::Component, Config, Engine, OptLevel};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: host-probe <component.wasm> <fuel:true|false> <opt:speed|speed-and-size|none>")?;
    let metered: bool = args.next().ok_or("missing fuel argument")?.parse()?;
    let mode = args.next().ok_or("missing optimization argument")?;
    if args.next().is_some() { return Err("unexpected extra argument".into()); }
    let level = match mode.as_str() {
        "speed" => OptLevel::Speed,
        "speed-and-size" => OptLevel::SpeedAndSize,
        "none" => OptLevel::None,
        _ => return Err(format!("unknown optimization mode {mode}").into()),
    };

    let bytes = std::fs::read(&path)?;
    eprintln!("Windows ARM64 Wasmtime Rust host: compiling {path} ({} bytes), fuel={metered}, optimization={mode}", bytes.len());
    let mut config = Config::new();
    config.consume_fuel(metered);
    config.wasm_memory64(false);
    config.cranelift_opt_level(level);
    let engine = Engine::new(&config)?;
    let _component = Component::new(&engine, bytes.as_slice())?;
    eprintln!("Windows ARM64 Wasmtime Rust host: compiled fuel={metered}, optimization={mode}");
    Ok(())
}
