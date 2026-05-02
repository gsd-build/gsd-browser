#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureConsumer {
    LocalViewer,
    CloudFrame,
    Screenshot,
    Refs,
    Dom,
    Accessibility,
    Logs,
    Recording,
    DebugBundle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureDecision {
    Allowed,
    RedactedFrameCard,
    OmitPayload,
    PartitionLogs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputActor {
    Agent,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDecision {
    Allowed,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct PrivacyPolicy {
    pub sensitive: bool,
    pub epoch: u64,
}

impl PrivacyPolicy {
    pub fn normal(epoch: u64) -> Self {
        Self {
            sensitive: false,
            epoch,
        }
    }

    pub fn sensitive(epoch: u64) -> Self {
        Self {
            sensitive: true,
            epoch,
        }
    }

    pub fn capture_decision(&self, consumer: CaptureConsumer) -> CaptureDecision {
        if !self.sensitive {
            return CaptureDecision::Allowed;
        }
        match consumer {
            CaptureConsumer::LocalViewer => CaptureDecision::Allowed,
            CaptureConsumer::CloudFrame => CaptureDecision::RedactedFrameCard,
            CaptureConsumer::Logs => CaptureDecision::PartitionLogs,
            CaptureConsumer::Screenshot
            | CaptureConsumer::Refs
            | CaptureConsumer::Dom
            | CaptureConsumer::Accessibility
            | CaptureConsumer::Recording
            | CaptureConsumer::DebugBundle => CaptureDecision::OmitPayload,
        }
    }

    pub fn input_decision(&self, actor: InputActor) -> InputDecision {
        if self.sensitive && actor == InputActor::Agent {
            InputDecision::Blocked
        } else {
            InputDecision::Allowed
        }
    }
}

pub fn policy_from_control(
    control: &gsd_browser_common::viewer::SharedControlStateV1,
) -> PrivacyPolicy {
    if control.sensitive {
        PrivacyPolicy::sensitive(control.control_version)
    } else {
        PrivacyPolicy::normal(control.control_version)
    }
}

pub fn redact_annotation(
    mut annotation: gsd_browser_common::viewer::AnnotationV1,
) -> gsd_browser_common::viewer::AnnotationV1 {
    if annotation
        .partial_reasons
        .iter()
        .any(|reason| reason == "sensitive_redacted")
    {
        annotation.target = None;
        annotation.artifact_refs = serde_json::json!({});
    }
    annotation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_policy_blocks_agent_capture() {
        let policy = PrivacyPolicy::sensitive(3);
        assert_eq!(
            policy.capture_decision(CaptureConsumer::CloudFrame),
            CaptureDecision::RedactedFrameCard
        );
        assert_eq!(
            policy.capture_decision(CaptureConsumer::DebugBundle),
            CaptureDecision::OmitPayload
        );
        assert_eq!(
            policy.input_decision(InputActor::Agent),
            InputDecision::Blocked
        );
        assert_eq!(
            policy.input_decision(InputActor::User),
            InputDecision::Allowed
        );
    }

    #[test]
    fn normal_policy_allows_local_capture() {
        let policy = PrivacyPolicy::normal(1);
        assert_eq!(
            policy.capture_decision(CaptureConsumer::LocalViewer),
            CaptureDecision::Allowed
        );
        assert_eq!(
            policy.input_decision(InputActor::Agent),
            InputDecision::Allowed
        );
    }
}
