use std::path::Path;

// ---------------------------------------------------------------------------
// Test Case 1: Workspace Scaffolding Abort Guardrail Validation
// ---------------------------------------------------------------------------

#[test]
fn test_root_path_guardrail_rejects_system_root() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(msg) = cursed_coder::guard::validate_workspace_root(Path::new("/")) {
        assert!(
            msg.contains("Execution prohibited within host system root paths"),
            "unexpected error message: {msg}"
        );
    } else {
        panic!("expected Err for root path, got Ok");
    }
    Ok(())
}

#[test]
fn test_root_path_guardrail_rejects_system_paths() -> Result<(), Box<dyn std::error::Error>> {
    let path = if cfg!(target_os = "windows") {
        Path::new("C:\\Windows")
    } else {
        Path::new("/etc")
    };
    match cursed_coder::guard::validate_workspace_root(path) {
        Err(msg) => assert!(msg.contains("host system root paths")),
        Ok(_) => panic!("expected Err for {path:?}, got Ok"),
    }
    Ok(())
}

#[test]
fn test_root_path_guardrail_accepts_safe_path() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let result = cursed_coder::guard::validate_workspace_root(dir.path());
    assert!(result.is_ok(), "expected Ok for temp dir, got Err");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test Case 2: Sandbox Workspace Isolation (init Subcommand Check)
// ---------------------------------------------------------------------------

#[test]
fn test_init_creates_files_in_isolation() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;

    cursed_coder::engine::initialize_workspace(dir.path())?;

    let steps = dir.path().join(".cursedcoder").join("steps.toml");
    let tasks = dir.path().join(".cursedcoder").join("tasks.toml");

    assert!(
        steps.exists(),
        "steps.toml was not created inside the sandbox"
    );
    assert!(
        tasks.exists(),
        "tasks.toml was not created inside the sandbox"
    );

    let steps_content = std::fs::read_to_string(&steps)?;
    assert!(
        steps_content.contains("[[step]]"),
        "steps.toml does not contain expected TOML structure"
    );

    Ok(())
}

#[test]
fn test_init_does_not_leak_to_host_env() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let env_key = "CURSEDCODER_TEST_SENTINEL";

    unsafe {
        std::env::remove_var(env_key);
    }
    assert!(
        std::env::var(env_key).is_err(),
        "sentinel should be absent before test"
    );

    cursed_coder::engine::initialize_workspace(dir.path())?;

    // Host env must remain unmodified after initialization
    assert!(
        std::env::var(env_key).is_err(),
        "host environment was polluted by init"
    );

    assert!(
        dir.path().join(".cursedcoder").join("steps.toml").exists(),
        "sandbox files should exist"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Test Case 3: Volatile Short-Term Memory Isolation
// ---------------------------------------------------------------------------

#[test]
fn test_variable_substitution_from_memory() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = tempfile::tempdir()?;
    let mut memory = cursed_coder::memory::Memory::load_or_create(dir.path())?;

    memory.set_variable(
        "COMPUTED_HASH",
        serde_json::Value::String("0xDEADBEEF".to_string()),
    );

    let template = "The hash value is {COMPUTED_HASH}";
    let result = cursed_coder::handlers::resolve_template(template, &memory)?;

    assert_eq!(result, "The hash value is 0xDEADBEEF");

    Ok(())
}

#[test]
fn test_variable_substitution_missing_key_errors() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let memory = cursed_coder::memory::Memory::load_or_create(dir.path())?;

    match cursed_coder::handlers::resolve_template("hello {MISSING}", &memory) {
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("MISSING"), "error should mention the missing key: {msg}");
        }
        Ok(_) => panic!("expected Err for missing variable"),
    }

    Ok(())
}

#[test]
fn test_variable_substitution_unclosed_placeholder_errors(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let memory = cursed_coder::memory::Memory::load_or_create(dir.path())?;

    let result = cursed_coder::handlers::resolve_template("hello {WORLD", &memory);
    assert!(result.is_err(), "expected Err for unclosed placeholder");

    Ok(())
}
