use gsd_browser_common::{
    cloud::{CloudFrame, CloudUserInput},
    identity::{identity_profile_dir, IdentityScope},
};
use serde_json::{json, Value};

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

#[test]
fn cloud_frame_contract_includes_viewport_metadata() {
    let frame = CloudFrame {
        sequence: 7,
        content_type: "image/jpeg".to_string(),
        data_base64: "abc".to_string(),
        width: 1280,
        height: 720,
        viewport_width: 640,
        viewport_height: 360,
        device_pixel_ratio: 2.0,
        captured_at_ms: 123,
        url: "https://preview.gsd.local".to_string(),
        title: "Preview".to_string(),
    };

    let value = serde_json::to_value(frame).expect("serialize frame");

    assert_eq!(value["sequence"], 7);
    assert_eq!(value["viewportWidth"], 640);
    assert_eq!(value["viewportHeight"], 360);
    assert_eq!(value["devicePixelRatio"], 2.0);
}

#[test]
fn cloud_user_input_accepts_web_surface_metadata() {
    let input: CloudUserInput = serde_json::from_value(json!({
        "kind": "pointer_down",
        "owner": "user",
        "controlVersion": 4,
        "frameSequence": 9,
        "coordinateSpace": "viewport-css-px",
        "x": 12.5,
        "y": 24.25,
        "button": "left",
        "modifiers": ["Shift", "Meta"]
    }))
    .expect("deserialize user input");

    assert_eq!(input.kind, "pointer_down");
    assert_eq!(input.owner.as_deref(), Some("user"));
    assert_eq!(input.control_version, Some(4));
    assert_eq!(input.frame_sequence, Some(9));
    assert_eq!(input.coordinate_space.as_deref(), Some("viewport-css-px"));
    assert_eq!(input.x, Some(12.5));
    assert_eq!(input.y, Some(24.25));
    assert_eq!(input.button.as_deref(), Some("left"));
    assert_eq!(
        input.modifiers,
        Some(vec!["Shift".to_string(), "Meta".to_string()])
    );

    let rendered: Value = serde_json::to_value(input).expect("serialize user input");
    assert_eq!(rendered["controlVersion"], 4);
    assert_eq!(rendered["frameSequence"], 9);
    assert_eq!(rendered["coordinateSpace"], "viewport-css-px");
}
