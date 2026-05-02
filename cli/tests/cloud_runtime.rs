#[test]
fn cli_rejects_identity_project_without_project_scope() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_gsd-browser"))
        .args([
            "--identity-scope",
            "global",
            "--identity-key",
            "github",
            "--identity-project",
            "project_1",
            "cloud-methods",
            "--json",
        ])
        .output()
        .expect("run gsd-browser");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--identity-project is only valid"));
}

#[test]
fn cloud_methods_manifest_advertises_input_and_identity_capabilities() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_gsd-browser"))
        .args([
            "--identity-scope",
            "project",
            "--identity-key",
            "github",
            "--identity-project",
            "project_1",
            "cloud-methods",
            "--json",
        ])
        .output()
        .expect("run gsd-browser");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("manifest json");
    assert_eq!(manifest["input"]["coordinateSpace"], "viewport_css");
    assert_eq!(manifest["identity"]["localFirst"], true);
    assert!(manifest["identity"]["scopes"]
        .as_array()
        .expect("scopes")
        .iter()
        .any(|scope| scope == "project"));
}
