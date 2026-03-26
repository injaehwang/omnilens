// Main command
pub mod analyze;

// Supporting commands (AI calls these internally)
pub mod check;
pub mod fix;
pub mod status;
pub mod hook;
pub mod ci;

// Advanced commands (AI agents & power users)
pub mod init;
pub mod index;
pub mod impact;
pub mod verify;
pub mod query;
pub mod testgen;
pub mod trace;
pub mod graph;
pub mod serve;
pub mod invariants;

use omnilens_core::Engine;
use omnilens_frontend_python::PythonFrontend;
use omnilens_frontend_rust::RustFrontend;
use omnilens_frontend_typescript::TypeScriptFrontend;

/// Create an engine with all compiled-in frontends registered.
pub fn create_engine() -> anyhow::Result<Engine> {
    let cwd = std::env::current_dir()?;
    let mut engine = Engine::init(&cwd)?;
    engine.register_frontend(Box::new(RustFrontend::new()));
    engine.register_frontend(Box::new(TypeScriptFrontend::new()));
    engine.register_frontend(Box::new(PythonFrontend::new()));
    Ok(engine)
}
