use gsd_browser_common::viewer::{
    ControlMode, ControlOwner, SharedControlStateV1, UserInputEventV1,
};

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
        }
    }

    pub fn snapshot(&self) -> SharedControlStateV1 {
        self.state.clone()
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
        self.bump();
        Ok(self.snapshot())
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
    store
        .authorize(AuthorizationRequest {
            owner: input.owner.clone(),
            control_version: input.control_version,
            frame_seq: input.frame_seq,
            effect: PageEffect::Input,
        })
        .map_err(|err| format!("control rejected: {:?}", err.reason))
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
}
