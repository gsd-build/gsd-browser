use gsd_browser_common::cloud::CloudUserInput;
use serde_json::json;

#[test]
fn cloud_user_input_accepts_viewport_css_pointer_payload() {
    let payload = json!({
        "kind": "pointer",
        "phase": "down",
        "x": 42.0,
        "y": 24.0,
        "button": "left",
        "buttons": 1,
        "clickCount": 1,
        "pointerType": "mouse",
        "coordinateSpace": "viewport_css",
        "frameSeq": 10,
        "controlVersion": 4
    });
    let input: CloudUserInput = serde_json::from_value(payload).expect("valid cloud input");
    assert_eq!(input.coordinate_space.as_deref(), Some("viewport_css"));
    assert_eq!(input.phase.as_deref(), Some("down"));
}

#[test]
fn cloud_user_input_rejects_invalid_coordinate_space() {
    let err = serde_json::from_value::<CloudUserInput>(json!({
        "kind": "pointer",
        "phase": "click",
        "coordinateSpace": "frame_css_pixels",
        "x": 1,
        "y": 1
    }))
    .expect_err("invalid coordinate space");
    assert!(err.to_string().contains("coordinateSpace"));
}

#[test]
fn cloud_user_input_rejects_missing_or_unknown_kind() {
    assert!(serde_json::from_value::<CloudUserInput>(json!({ "phase": "click" })).is_err());
    assert!(serde_json::from_value::<CloudUserInput>(json!({ "kind": "tap" })).is_err());
}

#[test]
fn cloud_user_input_rejects_invalid_pointer_phase_and_click_count() {
    assert!(serde_json::from_value::<CloudUserInput>(json!({
        "kind": "pointer",
        "phase": "teleport",
        "coordinateSpace": "viewport_css",
        "x": 1,
        "y": 1
    }))
    .is_err());
    assert!(serde_json::from_value::<CloudUserInput>(json!({
        "kind": "pointer",
        "phase": "click",
        "coordinateSpace": "viewport_css",
        "clickCount": -1,
        "x": 1,
        "y": 1
    }))
    .is_err());
}

#[test]
fn cloud_user_input_rejects_navigation_with_invalid_url() {
    assert!(serde_json::from_value::<CloudUserInput>(json!({
        "kind": "navigation",
        "url": "javascript:alert(1)"
    }))
    .is_err());
}
