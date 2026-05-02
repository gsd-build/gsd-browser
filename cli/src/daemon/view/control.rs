use gsd_browser_common::viewer::{
    ApprovalRequestV1, ControlMode, ControlOwner, SharedControlStateV1, UserInputEventV1,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEffect {
    Input,
    Observe,
    Export,
    Annotation,
    Recording,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEffectSource {
    Cloud,
    Viewer,
    Cli,
}

#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub owner: ControlOwner,
    pub control_version: u64,
    pub frame_seq: u64,
    pub effect: PageEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlRejectReason {
    StaleControlVersion,
    StaleFrameSeq,
    NonOwnerInput,
    AgentNotAllowedWhilePaused,
    AnnotationModeBlocksPageInput,
    SensitivePrivacyMode,
}

#[derive(Debug, Clone)]
pub struct ControlReject {
    pub reason: ControlRejectReason,
}

pub struct SharedControlStore {
    state: SharedControlStateV1,
    pending_approval: Option<ApprovalRequestV1>,
    pending_input: Option<UserInputEventV1>,
    approved_input: Option<UserInputEventV1>,
    approved_command_hash: Option<String>,
}

impl SharedControlStore {
    pub fn new() -> Self {
        Self::new_for_tests(1, 1)
    }

    pub fn new_for_tests(control_version: u64, frame_seq: u64) -> Self {
        Self {
            state: SharedControlStateV1 {
                owner: ControlOwner::Agent,
                mode: ControlMode::AgentRunning,
                control_version,
                frame_seq,
                requested_by: None,
                expires_at_ms: None,
                sensitive: false,
                reason: String::new(),
            },
            pending_approval: None,
            pending_input: None,
            approved_input: None,
            approved_command_hash: None,
        }
    }

    pub fn snapshot(&self) -> SharedControlStateV1 {
        self.state.clone()
    }

    pub fn pending_approval(&self) -> Option<ApprovalRequestV1> {
        self.pending_approval.clone()
    }

    fn bump(&mut self) {
        self.state.control_version += 1;
    }

    pub fn authorize(
        &mut self,
        req: AuthorizationRequest,
    ) -> Result<SharedControlStateV1, ControlReject> {
        if req.control_version != self.state.control_version {
            return Err(ControlReject {
                reason: ControlRejectReason::StaleControlVersion,
            });
        }
        if req.frame_seq < self.state.frame_seq {
            return Err(ControlReject {
                reason: ControlRejectReason::StaleFrameSeq,
            });
        }
        if self.state.mode == ControlMode::Annotating && req.effect == PageEffect::Input {
            return Err(ControlReject {
                reason: ControlRejectReason::AnnotationModeBlocksPageInput,
            });
        }
        if self.state.sensitive
            && req.owner == ControlOwner::Agent
            && req.effect == PageEffect::Input
        {
            return Err(ControlReject {
                reason: ControlRejectReason::SensitivePrivacyMode,
            });
        }
        if self.state.mode == ControlMode::Paused
            && req.owner == ControlOwner::Agent
            && req.effect == PageEffect::Input
        {
            return Err(ControlReject {
                reason: ControlRejectReason::AgentNotAllowedWhilePaused,
            });
        }
        if self.state.mode == ControlMode::Step
            && req.owner == ControlOwner::Agent
            && req.effect == PageEffect::Input
        {
            self.state.mode = ControlMode::Paused;
            self.bump();
            return Ok(self.snapshot());
        }
        if req.effect == PageEffect::Input && req.owner != self.state.owner {
            return Err(ControlReject {
                reason: ControlRejectReason::NonOwnerInput,
            });
        }
        Ok(self.snapshot())
    }

    pub fn takeover(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.owner = ControlOwner::User;
        self.state.mode = ControlMode::UserTakeover;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }

    pub fn release(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.owner = ControlOwner::Agent;
        self.state.mode = ControlMode::AgentRunning;
        self.state.reason = reason.into();
        self.state.sensitive = false;
        self.bump();
        Ok(self.snapshot())
    }

    pub fn pause(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.mode = ControlMode::Paused;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }

    pub fn step(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.mode = ControlMode::Step;
        self.state.owner = ControlOwner::Agent;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }

    pub fn annotate(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.owner = ControlOwner::User;
        self.state.mode = ControlMode::Annotating;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }

    pub fn abort(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.state.mode = ControlMode::Aborted;
        self.state.reason = reason.into();
        self.pending_approval = None;
        self.approved_command_hash = None;
        self.bump();
        Ok(self.snapshot())
    }

    pub fn request_approval(
        &mut self,
        request: ApprovalRequestV1,
    ) -> Result<SharedControlStateV1, String> {
        self.request_approval_with_input(request, None)
    }

    pub fn request_approval_with_input(
        &mut self,
        request: ApprovalRequestV1,
        input: Option<UserInputEventV1>,
    ) -> Result<SharedControlStateV1, String> {
        self.state.mode = ControlMode::ApprovalRequired;
        self.state.reason = request.summary.clone();
        self.state.requested_by = Some("risk-gate".to_string());
        self.state.expires_at_ms = Some(request.expires_at_ms);
        self.pending_approval = Some(request);
        self.pending_input = input;
        self.bump();
        Ok(self.snapshot())
    }

    pub fn approve(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        let pending = self.pending_approval.take().ok_or("no pending approval")?;
        if pending.expires_at_ms <= now_ms() {
            self.state.mode = ControlMode::UserTakeover;
            self.state.reason = "approval expired".to_string();
            self.bump();
            return Err("approval expired".to_string());
        }
        self.approved_command_hash = Some(pending.command_hash);
        self.approved_input = self.pending_input.take();
        self.state.owner = ControlOwner::User;
        self.state.mode = ControlMode::UserTakeover;
        self.state.reason = reason.into();
        self.state.expires_at_ms = None;
        self.state.requested_by = None;
        self.bump();
        Ok(self.snapshot())
    }

    pub fn deny(&mut self, reason: impl Into<String>) -> Result<SharedControlStateV1, String> {
        self.pending_approval = None;
        self.pending_input = None;
        self.approved_input = None;
        self.approved_command_hash = None;
        self.state.owner = ControlOwner::User;
        self.state.mode = ControlMode::UserTakeover;
        self.state.reason = reason.into();
        self.state.expires_at_ms = None;
        self.state.requested_by = None;
        self.bump();
        Ok(self.snapshot())
    }

    pub fn expire_approval(&mut self) -> Result<SharedControlStateV1, String> {
        self.pending_approval = None;
        self.pending_input = None;
        self.approved_input = None;
        self.approved_command_hash = None;
        self.state.mode = ControlMode::UserTakeover;
        self.state.reason = "approval expired".to_string();
        self.state.expires_at_ms = None;
        self.bump();
        Ok(self.snapshot())
    }

    pub fn consume_approval(&mut self, command_hash: &str) -> bool {
        if self
            .approved_command_hash
            .as_deref()
            .is_some_and(|approved| approved == command_hash)
        {
            self.approved_command_hash = None;
            self.approved_input = None;
            return true;
        }
        false
    }

    pub fn take_approved_input(&mut self) -> Option<UserInputEventV1> {
        self.approved_command_hash = None;
        self.approved_input.take()
    }

    pub fn sensitive_on(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<SharedControlStateV1, String> {
        self.state.owner = ControlOwner::User;
        self.state.mode = ControlMode::Sensitive;
        self.state.sensitive = true;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }

    pub fn sensitive_off(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<SharedControlStateV1, String> {
        self.state.mode = ControlMode::UserTakeover;
        self.state.sensitive = false;
        self.state.reason = reason.into();
        self.bump();
        Ok(self.snapshot())
    }
}

pub async fn authorize_page_effect(
    state: &crate::daemon::state::DaemonState,
    _source: PageEffectSource,
    input: &UserInputEventV1,
) -> Result<SharedControlStateV1, String> {
    let mut store = state.view_control.lock().await;
    let control = store
        .authorize(AuthorizationRequest {
            owner: input.owner.clone(),
            control_version: input.control_version,
            frame_seq: input.frame_seq,
            effect: PageEffect::Input,
        })
        .map_err(|err| format!("control rejected: {:?}", err.reason))?;

    let risk = risk_input_for_command(state, input);
    let evaluation = crate::daemon::view::risk::evaluate_risk(risk);
    if evaluation.requires_approval {
        let command_hash = crate::daemon::view::risk::command_hash(input);
        if store.consume_approval(&command_hash) {
            return Ok(control);
        }
        let expires_at_ms = now_ms() + 30_000;
        if let Some(request) = evaluation.approval_request(
            command_hash,
            input.url.clone().unwrap_or_default(),
            expires_at_ms,
        ) {
            let _ = store.request_approval_with_input(request, Some(input.clone()));
        }
        return Err("control rejected: ApprovalRequired".to_string());
    }

    Ok(control)
}

fn risk_input_for_command(
    state: &crate::daemon::state::DaemonState,
    input: &UserInputEventV1,
) -> crate::daemon::view::risk::RiskInput {
    let mut role = None;
    let mut name = None;
    if input.kind == gsd_browser_common::viewer::UserInputKind::Pointer {
        if let (Some(x), Some(y)) = (input.x, input.y) {
            if let Some(target) = ref_at_point(state, x, y) {
                role = target
                    .get("role")
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
                name = target
                    .get("name")
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
            }
        }
    }
    crate::daemon::view::risk::RiskInput {
        origin: input
            .url
            .as_deref()
            .map(origin_from_url)
            .unwrap_or_default(),
        role,
        name,
        text: input.text.clone().or_else(|| input.action.clone()),
        input_kind: format!("{:?}", input.kind).to_lowercase(),
        url: input.url.clone().unwrap_or_default(),
    }
}

fn ref_at_point(
    state: &crate::daemon::state::DaemonState,
    x: f64,
    y: f64,
) -> Option<serde_json::Value> {
    let refs = state.refs.lock().ok()?;
    refs.refs.values().find_map(|node| {
        let left = node.get("x")?.as_f64()?;
        let top = node.get("y")?.as_f64()?;
        let width = node.get("w")?.as_f64()?;
        let height = node.get("h")?.as_f64()?;
        (x >= left && x <= left + width && y >= top && y <= top + height).then(|| node.clone())
    })
}

fn origin_from_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return String::new();
    };
    let host = rest.split('/').next().unwrap_or_default();
    format!("{scheme}://{host}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use gsd_browser_common::viewer::{ControlMode, ControlOwner};

    #[test]
    fn stale_control_version_rejects_ahead_of_owner_check() {
        let mut store = SharedControlStore::new_for_tests(10, 4);
        let req = AuthorizationRequest {
            owner: ControlOwner::User,
            control_version: 9,
            frame_seq: 4,
            effect: PageEffect::Input,
        };
        let err = store.authorize(req).expect_err("stale command");
        assert_eq!(err.reason, ControlRejectReason::StaleControlVersion);
    }

    #[test]
    fn stale_frame_rejects_ahead_of_owner_check() {
        let mut store = SharedControlStore::new_for_tests(10, 4);
        let req = AuthorizationRequest {
            owner: ControlOwner::User,
            control_version: 10,
            frame_seq: 3,
            effect: PageEffect::Input,
        };
        let err = store.authorize(req).expect_err("stale frame");
        assert_eq!(err.reason, ControlRejectReason::StaleFrameSeq);
    }

    #[test]
    fn takeover_preempts_agent_and_increments_version() {
        let mut store = SharedControlStore::new_for_tests(1, 1);
        let state = store.takeover("manual test").expect("takeover");
        assert_eq!(state.owner, ControlOwner::User);
        assert_eq!(state.mode, ControlMode::UserTakeover);
        assert_eq!(state.control_version, 2);
    }

    #[test]
    fn step_accepts_one_agent_effect_then_pauses() {
        let mut store = SharedControlStore::new_for_tests(1, 1);
        store.pause("inspect").expect("pause");
        store.step("allow one").expect("step");
        let accepted = store
            .authorize(AuthorizationRequest {
                owner: ControlOwner::Agent,
                control_version: 3,
                frame_seq: 1,
                effect: PageEffect::Input,
            })
            .expect("one action");
        assert_eq!(accepted.mode, ControlMode::Paused);

        let err = store
            .authorize(AuthorizationRequest {
                owner: ControlOwner::Agent,
                control_version: 4,
                frame_seq: 1,
                effect: PageEffect::Input,
            })
            .expect_err("paused blocks agent");
        assert_eq!(err.reason, ControlRejectReason::AgentNotAllowedWhilePaused);
    }

    #[test]
    fn annotating_blocks_page_input() {
        let mut store = SharedControlStore::new_for_tests(1, 1);
        store.annotate("select UI").expect("annotating");
        let err = store
            .authorize(AuthorizationRequest {
                owner: ControlOwner::User,
                control_version: 2,
                frame_seq: 1,
                effect: PageEffect::Input,
            })
            .expect_err("input blocked");
        assert_eq!(
            err.reason,
            ControlRejectReason::AnnotationModeBlocksPageInput
        );
    }

    #[test]
    fn approval_consumes_exact_pending_command_hash() {
        let mut store = SharedControlStore::new_for_tests(1, 1);
        store
            .request_approval(ApprovalRequestV1 {
                approval_id: "approval_abc".to_string(),
                command_hash: "abc".to_string(),
                summary: "Delete requires approval".to_string(),
                origin: "https://app.example.com".to_string(),
                expires_at_ms: now_ms() + 1_000,
                risk: serde_json::json!({ "category": "delete_destructive" }),
            })
            .expect("approval requested");
        store.approve("approved").expect("approved");
        assert!(!store.consume_approval("other"));
        assert!(store.consume_approval("abc"));
        assert!(!store.consume_approval("abc"));
    }
}
