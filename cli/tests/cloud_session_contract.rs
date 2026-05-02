use gsd_browser_common::{
    cloud::{CloudFrame, CloudRef, CloudRefs, CloudToolRequest, CloudUserInput},
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
        viewport_css_width: 640,
        viewport_css_height: 360,
        capture_pixel_width: 1280,
        capture_pixel_height: 720,
        device_pixel_ratio: 2.0,
        capture_scale_x: 2.0,
        capture_scale_y: 2.0,
        captured_at_ms: 123,
        encoded_bytes: 3,
        quality: 70,
        capture_pixel_ratio: 2.0,
        url: "https://preview.gsd.local".to_string(),
        title: "Preview".to_string(),
    };

    let value = serde_json::to_value(frame).expect("serialize frame");

    assert_eq!(value["sequence"], 7);
    assert_eq!(value["viewportWidth"], 640);
    assert_eq!(value["viewportHeight"], 360);
    assert_eq!(value["viewportCssWidth"], 640);
    assert_eq!(value["viewportCssHeight"], 360);
    assert_eq!(value["capturePixelWidth"], 1280);
    assert_eq!(value["capturePixelHeight"], 720);
    assert_eq!(value["devicePixelRatio"], 2.0);
    assert_eq!(value["captureScaleX"], 2.0);
    assert_eq!(value["captureScaleY"], 2.0);
    assert_eq!(value["encodedBytes"], 3);
    assert_eq!(value["quality"], 70);
    assert_eq!(value["capturePixelRatio"], 2.0);
}

#[test]
fn cloud_user_input_accepts_web_surface_metadata() {
    let input: CloudUserInput = serde_json::from_value(json!({
        "kind": "pointer",
        "phase": "down",
        "owner": "user",
        "controlVersion": 4,
        "frameSeq": 9,
        "coordinateSpace": "viewport_css",
        "x": 12.5,
        "y": 24.25,
        "button": "left",
        "modifiers": ["Shift", "Meta"]
    }))
    .expect("deserialize user input");

    assert_eq!(input.kind, "pointer");
    assert_eq!(input.phase.as_deref(), Some("down"));
    assert_eq!(input.owner.as_deref(), Some("user"));
    assert_eq!(input.control_version, Some(4));
    assert_eq!(input.frame_seq, Some(9));
    assert_eq!(input.coordinate_space.as_deref(), Some("viewport_css"));
    assert_eq!(input.x, Some(12.5));
    assert_eq!(input.y, Some(24.25));
    assert_eq!(input.button.as_deref(), Some("left"));
    assert_eq!(
        input.modifiers,
        Some(vec!["Shift".to_string(), "Meta".to_string()])
    );

    let rendered: Value = serde_json::to_value(input).expect("serialize user input");
    assert_eq!(rendered["controlVersion"], 4);
    assert_eq!(rendered["frameSeq"], 9);
    assert_eq!(rendered["coordinateSpace"], "viewport_css");
}

#[test]
fn cloud_tool_request_serializes_expanded_method_names() {
    let request = CloudToolRequest {
        method: "check_injection".to_string(),
        params: serde_json::json!({ "includeHidden": true }),
    };

    let value = serde_json::to_value(request).expect("serialize request");

    assert_eq!(value["method"], "check_injection");
    assert_eq!(value["params"]["includeHidden"], true);
}

#[test]
fn cloud_methods_manifest_contains_known_methods() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_gsd-browser"))
        .args(["cloud-methods", "--json"])
        .output()
        .expect("run gsd-browser cloud-methods");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("manifest json");
    assert_eq!(manifest["manifestVersion"], 1);
    assert_eq!(manifest["runtimeMinVersion"], "0.1.24");
    assert_eq!(manifest["input"]["coordinateSpace"], "viewport_css");
    assert_eq!(manifest["identity"]["localFirst"], true);

    let methods = manifest["methods"].as_array().expect("methods");
    let method_names: std::collections::BTreeSet<_> = methods
        .iter()
        .map(|method| method["name"].as_str().expect("method name"))
        .collect();
    assert!(method_names.contains("navigate"));
    assert!(method_names.contains("snapshot"));
    assert!(method_names.contains("click_ref"));
    assert!(method_names.contains("visual_diff"));

    for method in methods {
        assert!(method["category"].as_str().expect("category").len() > 0);
    }
}

#[test]
fn cloud_refs_contract_uses_numeric_version_and_rendered_refs() {
    let refs = CloudRefs {
        version: 3,
        refs: vec![CloudRef {
            ref_id: "@v3:e1".to_string(),
            key: "e1".to_string(),
            role: "button".to_string(),
            name: Some("Submit".to_string()),
            x: 12.5,
            y: 24.5,
            w: 80.0,
            h: 32.0,
        }],
        truncated: true,
        limit: Some(200),
        captured_at_ms: 123,
    };

    let value = serde_json::to_value(refs).expect("serialize refs");

    assert_eq!(value["version"], 3);
    assert_eq!(value["refs"][0]["ref"], "@v3:e1");
    assert_eq!(value["refs"][0]["key"], "e1");
    assert_eq!(value["truncated"], true);
    assert_eq!(value["limit"], 200);
    assert_eq!(value["capturedAtMs"], 123);
}
