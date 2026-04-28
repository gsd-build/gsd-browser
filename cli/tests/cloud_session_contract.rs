use gsd_browser_common::identity::{identity_profile_dir, IdentityScope};

#[test]
fn project_identity_profile_path_is_stable_and_local() {
    let path = identity_profile_dir(IdentityScope::Project, Some("project_123"), "acme-admin")
        .expect("identity path");

    let rendered = path.to_string_lossy();
    assert!(rendered.contains(".gsd-browser"));
    assert!(rendered.contains("identities"));
    assert!(rendered.contains("project_123"));
    assert!(rendered.contains("acme-admin"));
}

#[test]
fn identity_names_reject_path_traversal() {
    let err = identity_profile_dir(IdentityScope::Project, Some("project_123"), "../secret")
        .expect_err("invalid identity key");
    assert!(err.contains("invalid name"));

    let err = identity_profile_dir(IdentityScope::Project, Some("../secret"), "project_123")
        .expect_err("invalid project id");
    assert!(err.contains("invalid name"));
}
