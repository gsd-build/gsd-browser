use gsd_browser_common::viewer::{ApprovalRequestV1, UserInputEventV1};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskCategory {
    PurchasePayment,
    DeleteDestructive,
    SendInviteShare,
    OAuthGrant,
    CredentialTokenEntry,
    FileTransfer,
    ProductionAdminOrigin,
    CrossOriginNavigation,
    SensitiveFormField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskInput {
    pub origin: String,
    pub role: Option<String>,
    pub name: Option<String>,
    pub text: Option<String>,
    pub input_kind: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskEvaluation {
    pub category: Option<RiskCategory>,
    pub requires_approval: bool,
    pub summary: String,
}

impl RiskEvaluation {
    pub fn approval_request(
        &self,
        command_hash: String,
        origin: String,
        expires_at_ms: u64,
    ) -> Option<ApprovalRequestV1> {
        self.requires_approval.then(|| ApprovalRequestV1 {
            approval_id: format!("approval_{command_hash}"),
            command_hash,
            summary: self.summary.clone(),
            origin,
            expires_at_ms,
            risk: json!({
                "category": self.category,
                "summary": self.summary,
            }),
        })
    }
}

pub fn evaluate_risk(input: RiskInput) -> RiskEvaluation {
    let role = input.role.unwrap_or_default().to_lowercase();
    let name = input.name.unwrap_or_default().to_lowercase();
    let text = input.text.unwrap_or_default().to_lowercase();
    let input_kind = input.input_kind.to_lowercase();
    let url = input.url.to_lowercase();
    let origin = input.origin.to_lowercase();
    let haystack = format!("{role} {name} {text} {input_kind} {url} {origin}");

    let category = if contains_any(
        &haystack,
        &[
            "delete",
            "destroy",
            "remove project",
            "drop database",
            "terminate",
        ],
    ) {
        Some(RiskCategory::DeleteDestructive)
    } else if contains_any(
        &haystack,
        &[
            "purchase",
            "payment",
            "checkout",
            "subscribe",
            "billing",
            "invoice",
            "pay ",
        ],
    ) {
        Some(RiskCategory::PurchasePayment)
    } else if contains_any(&haystack, &["send", "invite", "share", "publish", "email"]) {
        Some(RiskCategory::SendInviteShare)
    } else if contains_any(
        &haystack,
        &["oauth", "authorize", "grant access", "consent"],
    ) {
        Some(RiskCategory::OAuthGrant)
    } else if contains_any(
        &haystack,
        &["password", "token", "secret", "api key", "otp"],
    ) {
        Some(RiskCategory::CredentialTokenEntry)
    } else if contains_any(&haystack, &["upload", "download", "export", "import file"]) {
        Some(RiskCategory::FileTransfer)
    } else if contains_any(
        &haystack,
        &["admin", "production", "prod", "fly.io", "vercel.com"],
    ) {
        Some(RiskCategory::ProductionAdminOrigin)
    } else if input_kind == "navigation" && is_cross_origin(&origin, &url) {
        Some(RiskCategory::CrossOriginNavigation)
    } else if contains_any(
        &haystack,
        &["credit card", "ssn", "social security", "private key"],
    ) {
        Some(RiskCategory::SensitiveFormField)
    } else {
        None
    };

    let summary = match &category {
        Some(RiskCategory::DeleteDestructive) => "Destructive action requires approval",
        Some(RiskCategory::PurchasePayment) => "Payment or purchase action requires approval",
        Some(RiskCategory::SendInviteShare) => "Outbound send/share action requires approval",
        Some(RiskCategory::OAuthGrant) => "OAuth grant requires approval",
        Some(RiskCategory::CredentialTokenEntry) => "Credential entry requires approval",
        Some(RiskCategory::FileTransfer) => "File transfer requires approval",
        Some(RiskCategory::ProductionAdminOrigin) => "Production/admin origin requires approval",
        Some(RiskCategory::CrossOriginNavigation) => "Cross-origin navigation requires approval",
        Some(RiskCategory::SensitiveFormField) => "Sensitive form field requires approval",
        None => "",
    }
    .to_string();

    RiskEvaluation {
        requires_approval: category.is_some(),
        category,
        summary,
    }
}

pub fn command_hash(input: &UserInputEventV1) -> String {
    let value = serde_json::json!({
        "kind": input.kind,
        "phase": input.phase,
        "x": input.x.map(|x| (x * 10.0).round() as i64),
        "y": input.y.map(|y| (y * 10.0).round() as i64),
        "text": input.text,
        "key": input.key,
        "button": input.button,
        "deltaX": input.delta_x.map(|x| (x * 10.0).round() as i64),
        "deltaY": input.delta_y.map(|y| (y * 10.0).round() as i64),
        "url": input.url,
        "action": input.action,
    });
    let mut hasher = DefaultHasher::new();
    value.to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn is_cross_origin(origin: &str, url: &str) -> bool {
    if origin.is_empty()
        || url.is_empty()
        || !(url.starts_with("http://") || url.starts_with("https://"))
    {
        return false;
    }
    !url.starts_with(origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_button_requires_approval() {
        let risk = evaluate_risk(RiskInput {
            origin: "https://app.example.com".to_string(),
            role: Some("button".to_string()),
            name: Some("Delete project".to_string()),
            text: Some("Delete project".to_string()),
            input_kind: "pointer".to_string(),
            url: "https://app.example.com/projects/acme".to_string(),
        });
        assert_eq!(risk.category, Some(RiskCategory::DeleteDestructive));
        assert!(risk.requires_approval);
    }

    #[test]
    fn plain_local_link_does_not_require_approval() {
        let risk = evaluate_risk(RiskInput {
            origin: "http://localhost:3000".to_string(),
            role: Some("link".to_string()),
            name: Some("Settings".to_string()),
            text: Some("Settings".to_string()),
            input_kind: "pointer".to_string(),
            url: "http://localhost:3000/home".to_string(),
        });
        assert_eq!(risk.category, None);
        assert!(!risk.requires_approval);
    }
}
