//! Rune VM runtime creation and sandboxing

use rune::{Context, Diagnostics, Source, Sources, Unit, Vm};
use std::path::Path;
use std::sync::Arc;

use crate::manifest::PluginPermissions;

/// Configuration for the Rune sandbox
///
/// Some fields are reserved for future sandbox enforcement.
#[derive(Debug, Clone)]
#[allow(dead_code)]
#[derive(Default)]
pub struct SandboxConfig {
    /// Allow network/HTTP operations
    pub network: bool,
    /// Allow filesystem operations
    pub filesystem: bool,
    /// Allowed filesystem paths (reserved for future sandbox enforcement)
    pub allowed_paths: Vec<String>,
    /// Allow running external commands (reserved for future sandbox enforcement)
    pub run_commands: bool,
    /// Allowed commands (reserved for future sandbox enforcement)
    pub allowed_commands: Vec<String>,
}


impl SandboxConfig {
    /// Create sandbox config from plugin permissions
    pub fn from_permissions(permissions: &PluginPermissions) -> Self {
        Self {
            network: permissions.network,
            filesystem: !permissions.filesystem.is_empty(),
            allowed_paths: permissions.filesystem.clone(),
            run_commands: !permissions.run_commands.is_empty(),
            allowed_commands: permissions.run_commands.clone(),
        }
    }
}

/// Create a Rune context with owlry API modules
pub fn create_context(sandbox: &SandboxConfig) -> Result<Context, rune::ContextError> {
    let mut context = Context::with_default_modules()?;

    // Add standard modules based on permissions
    if sandbox.network {
        log::debug!("Network access enabled for Rune plugin");
    }

    if sandbox.filesystem {
        log::debug!("Filesystem access enabled for Rune plugin");
    }

    // Add owlry API module
    context.install(crate::api::module()?)?;

    Ok(context)
}

/// Compile Rune source code into a Unit
pub fn compile_source(
    context: &Context,
    source_path: &Path,
) -> Result<Arc<Unit>, CompileError> {
    let source_content = std::fs::read_to_string(source_path)
        .map_err(|e| CompileError::Io(e.to_string()))?;

    let source_name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("init.rn");

    let mut sources = Sources::new();
    sources
        .insert(Source::new(source_name, &source_content).map_err(|e| CompileError::Compile(e.to_string()))?)
        .map_err(|e| CompileError::Compile(format!("Failed to insert source: {}", e)))?;

    let mut diagnostics = Diagnostics::new();

    let result = rune::prepare(&mut sources)
        .with_context(context)
        .with_diagnostics(&mut diagnostics)
        .build();

    match result {
        Ok(unit) => Ok(Arc::new(unit)),
        Err(e) => {
            // Collect error messages
            let mut error_msg = format!("Compilation failed: {}", e);
            for diagnostic in diagnostics.diagnostics() {
                error_msg.push_str(&format!("\n  {:?}", diagnostic));
            }
            Err(CompileError::Compile(error_msg))
        }
    }
}

/// Create a new Rune VM from compiled unit
pub fn create_vm(
    context: &Context,
    unit: Arc<Unit>,
) -> Result<Vm, CompileError> {
    let runtime = Arc::new(
        context.runtime()
            .map_err(|e| CompileError::Compile(format!("Failed to get runtime: {}", e)))?
    );
    Ok(Vm::new(runtime, unit))
}

/// Error type for compilation
#[derive(Debug)]
pub enum CompileError {
    Io(String),
    Compile(String),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Io(e) => write!(f, "IO error: {}", e),
            CompileError::Compile(e) => write!(f, "Compile error: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert!(!config.network);
        assert!(!config.filesystem);
        assert!(!config.run_commands);
    }

    #[test]
    fn test_sandbox_from_permissions() {
        let permissions = PluginPermissions {
            network: true,
            filesystem: vec!["~/.config".to_string()],
            run_commands: vec!["notify-send".to_string()],
        };
        let config = SandboxConfig::from_permissions(&permissions);
        assert!(config.network);
        assert!(config.filesystem);
        assert!(config.run_commands);
        assert_eq!(config.allowed_paths, vec!["~/.config"]);
        assert_eq!(config.allowed_commands, vec!["notify-send"]);
    }

    #[test]
    fn test_create_context() {
        let config = SandboxConfig::default();
        let context = create_context(&config);
        assert!(context.is_ok());
    }
}
