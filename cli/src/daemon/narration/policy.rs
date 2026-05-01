use crate::daemon::narration::events::TargetInfo;

pub const LEAD_MS_FAST: u32 = 120;
pub const LEAD_MS_SLOW: u32 = 500;
pub const LEAD_MS_NOTARGET: u32 = 80;

/// Pick lead time based on whether we have a target and whether it scrolled into view.
pub fn lead_for(target: Option<&TargetInfo>) -> u32 {
    match target {
        None => LEAD_MS_NOTARGET,
        Some(t) if t.scrolled => LEAD_MS_SLOW,
        Some(_) => LEAD_MS_FAST,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_target_uses_notarget_bucket() {
        assert_eq!(lead_for(None), LEAD_MS_NOTARGET);
    }

    #[test]
    fn scrolled_target_uses_slow_bucket() {
        let t = TargetInfo {
            scrolled: true,
            ..Default::default()
        };
        assert_eq!(lead_for(Some(&t)), LEAD_MS_SLOW);
    }

    #[test]
    fn onscreen_target_uses_fast_bucket() {
        let t = TargetInfo {
            scrolled: false,
            ..Default::default()
        };
        assert_eq!(lead_for(Some(&t)), LEAD_MS_FAST);
    }
}
