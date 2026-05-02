use gsd_browser_common::identity::{
    identity_context_profile_dir, identity_profile_dir, validate_identity_context, IdentityScope,
};

#[test]
fn identity_scope_requires_valid_key_and_context() {
    assert!(IdentityScope::parse("session").is_ok());
    assert!(IdentityScope::parse("project").is_ok());
    assert!(IdentityScope::parse("global").is_ok());
    assert!(IdentityScope::parse("team").is_err());
}

#[test]
fn session_identity_requires_session_context() {
    assert!(validate_identity_context(IdentityScope::Session, None, Some("session_1")).is_ok());
    assert!(validate_identity_context(IdentityScope::Session, Some("project_1"), None).is_err());
    assert!(validate_identity_context(IdentityScope::Session, None, None).is_err());
}

#[test]
fn project_identity_requires_project_context() {
    assert!(validate_identity_context(IdentityScope::Project, Some("project_1"), None).is_ok());
    assert!(validate_identity_context(IdentityScope::Project, None, Some("session_1")).is_err());
}

#[test]
fn global_identity_rejects_project_context() {
    assert!(validate_identity_context(IdentityScope::Global, None, None).is_ok());
    assert!(validate_identity_context(IdentityScope::Global, Some("project_1"), None).is_err());
}

#[test]
fn identity_keys_reject_path_unsafe_segments() {
    for key in [
        "",
        ".",
        "..",
        "../secret",
        "/absolute",
        "folder/key",
        "folder\\key",
        "folder%2fkey",
        "folder%5ckey",
        "folder:key",
    ] {
        assert!(
            identity_profile_dir(IdentityScope::Global, None, key).is_err(),
            "key should be rejected: {key}"
        );
    }
}

#[test]
fn context_profile_paths_include_required_scope_context() {
    let session_path =
        identity_context_profile_dir(IdentityScope::Session, None, Some("session_1"), "github")
            .expect("session identity path");
    assert!(session_path.to_string_lossy().contains("session_1"));

    let project_path =
        identity_context_profile_dir(IdentityScope::Project, Some("project_1"), None, "github")
            .expect("project identity path");
    assert!(project_path.to_string_lossy().contains("project_1"));
}
