#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudToolCategory {
    Navigation,
    Interaction,
    ArtifactGeneration,
    Inspection,
    ExternalEffect,
    NetworkMutation,
    CredentialAuth,
    Composite,
}

impl CloudToolCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Navigation => "navigation",
            Self::Interaction => "interaction",
            Self::ArtifactGeneration => "artifact_generation",
            Self::Inspection => "inspection",
            Self::ExternalEffect => "external_effect",
            Self::NetworkMutation => "network_mutation",
            Self::CredentialAuth => "credential_auth",
            Self::Composite => "composite",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudToolMethod {
    pub name: &'static str,
    pub category: CloudToolCategory,
}

pub const CLOUD_TOOL_METHODS: &[CloudToolMethod] = &[
    CloudToolMethod {
        name: "navigate",
        category: CloudToolCategory::Navigation,
    },
    CloudToolMethod {
        name: "back",
        category: CloudToolCategory::Navigation,
    },
    CloudToolMethod {
        name: "forward",
        category: CloudToolCategory::Navigation,
    },
    CloudToolMethod {
        name: "reload",
        category: CloudToolCategory::Navigation,
    },
    CloudToolMethod {
        name: "list_pages",
        category: CloudToolCategory::Navigation,
    },
    CloudToolMethod {
        name: "switch_page",
        category: CloudToolCategory::Navigation,
    },
    CloudToolMethod {
        name: "close_page",
        category: CloudToolCategory::Navigation,
    },
    CloudToolMethod {
        name: "list_frames",
        category: CloudToolCategory::Navigation,
    },
    CloudToolMethod {
        name: "select_frame",
        category: CloudToolCategory::Navigation,
    },
    CloudToolMethod {
        name: "click",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "type",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "press",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "hover",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "scroll",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "select_option",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "set_checked",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "drag",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "set_viewport",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "click_ref",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "hover_ref",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "fill_ref",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "emulate_device",
        category: CloudToolCategory::Interaction,
    },
    CloudToolMethod {
        name: "upload_file",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "debug_bundle",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "screenshot",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "zoom_region",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "save_pdf",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "visual_diff",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "generate_test",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "har_export",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "trace_start",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "trace_stop",
        category: CloudToolCategory::ArtifactGeneration,
    },
    CloudToolMethod {
        name: "snapshot",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "get_ref",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "accessibility_tree",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "find",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "page_source",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "assert",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "diff",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "wait_for",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "analyze_form",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "find_best",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "console",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "network",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "dialog",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "timeline",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "session_summary",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "extract",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "action_cache",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "check_injection",
        category: CloudToolCategory::Inspection,
    },
    CloudToolMethod {
        name: "eval",
        category: CloudToolCategory::ExternalEffect,
    },
    CloudToolMethod {
        name: "fill_form",
        category: CloudToolCategory::ExternalEffect,
    },
    CloudToolMethod {
        name: "act",
        category: CloudToolCategory::ExternalEffect,
    },
    CloudToolMethod {
        name: "mock_route",
        category: CloudToolCategory::NetworkMutation,
    },
    CloudToolMethod {
        name: "block_urls",
        category: CloudToolCategory::NetworkMutation,
    },
    CloudToolMethod {
        name: "clear_routes",
        category: CloudToolCategory::NetworkMutation,
    },
    CloudToolMethod {
        name: "save_state",
        category: CloudToolCategory::CredentialAuth,
    },
    CloudToolMethod {
        name: "restore_state",
        category: CloudToolCategory::CredentialAuth,
    },
    CloudToolMethod {
        name: "vault_save",
        category: CloudToolCategory::CredentialAuth,
    },
    CloudToolMethod {
        name: "vault_login",
        category: CloudToolCategory::CredentialAuth,
    },
    CloudToolMethod {
        name: "vault_list",
        category: CloudToolCategory::CredentialAuth,
    },
    CloudToolMethod {
        name: "batch",
        category: CloudToolCategory::Composite,
    },
];

pub fn cloud_tool_method(name: &str) -> Option<&'static CloudToolMethod> {
    CLOUD_TOOL_METHODS.iter().find(|method| method.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_tool_methods_are_exactly_registered() {
        let names = CLOUD_TOOL_METHODS
            .iter()
            .map(|method| method.name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "navigate",
                "back",
                "forward",
                "reload",
                "list_pages",
                "switch_page",
                "close_page",
                "list_frames",
                "select_frame",
                "click",
                "type",
                "press",
                "hover",
                "scroll",
                "select_option",
                "set_checked",
                "drag",
                "set_viewport",
                "click_ref",
                "hover_ref",
                "fill_ref",
                "emulate_device",
                "upload_file",
                "debug_bundle",
                "screenshot",
                "zoom_region",
                "save_pdf",
                "visual_diff",
                "generate_test",
                "har_export",
                "trace_start",
                "trace_stop",
                "snapshot",
                "get_ref",
                "accessibility_tree",
                "find",
                "page_source",
                "assert",
                "diff",
                "wait_for",
                "analyze_form",
                "find_best",
                "console",
                "network",
                "dialog",
                "timeline",
                "session_summary",
                "extract",
                "action_cache",
                "check_injection",
                "eval",
                "fill_form",
                "act",
                "mock_route",
                "block_urls",
                "clear_routes",
                "save_state",
                "restore_state",
                "vault_save",
                "vault_login",
                "vault_list",
                "batch",
            ]
        );
    }

    #[test]
    fn cloud_tool_method_categories_are_security_relevant() {
        assert_eq!(
            cloud_tool_method("eval").map(|method| method.category.as_str()),
            Some("external_effect")
        );
        assert_eq!(
            cloud_tool_method("batch").map(|method| method.category.as_str()),
            Some("composite")
        );
        assert_eq!(
            cloud_tool_method("vault_login").map(|method| method.category.as_str()),
            Some("credential_auth")
        );
        assert_eq!(cloud_tool_method("browser.navigate"), None);
    }
}
