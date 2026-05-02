use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;
pub const VIEWER_AUDIENCE: &str = "gsd-browser-viewer";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerTokenClaims {
    #[serde(rename = "aud")]
    pub audience: String,
    pub session_id: String,
    pub viewer_id: String,
    pub origin: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthRejectReason {
    MissingToken,
    MalformedToken,
    BadSignature,
    WrongSession,
    WrongViewer,
    WrongOrigin,
    ExpiredToken,
    NonLoopbackHost,
    CapabilityDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthReject {
    pub reason: AuthRejectReason,
}

#[derive(Clone)]
pub struct ViewerTokenIssuer {
    secret: [u8; 32],
}

impl ViewerTokenIssuer {
    pub fn new() -> Self {
        let mut secret = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        Self { secret }
    }

    pub fn new_for_tests(secret: [u8; 32]) -> Self {
        Self { secret }
    }

    pub fn default_ttl() -> Duration {
        Duration::from_secs(60 * 30)
    }

    pub fn issue(&self, claims: ViewerTokenClaims) -> Result<String, String> {
        let claims_json = serde_json::to_vec(&claims).map_err(|err| err.to_string())?;
        let mut mac = HmacSha256::new_from_slice(&self.secret).map_err(|err| err.to_string())?;
        mac.update(&claims_json);
        let signature = mac.finalize().into_bytes();
        Ok(format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(claims_json),
            URL_SAFE_NO_PAD.encode(signature)
        ))
    }

    pub fn verify(
        &self,
        token: &str,
        session_id: &str,
        viewer_id: &str,
        origin: &str,
        now_ms: u64,
        required_capability: Option<&str>,
    ) -> Result<ViewerTokenClaims, AuthReject> {
        let (claims_b64, sig_b64) = token.split_once('.').ok_or(AuthReject {
            reason: AuthRejectReason::MalformedToken,
        })?;
        let claims_json = URL_SAFE_NO_PAD.decode(claims_b64).map_err(|_| AuthReject {
            reason: AuthRejectReason::MalformedToken,
        })?;
        let signature = URL_SAFE_NO_PAD.decode(sig_b64).map_err(|_| AuthReject {
            reason: AuthRejectReason::MalformedToken,
        })?;

        let mut mac = HmacSha256::new_from_slice(&self.secret).map_err(|_| AuthReject {
            reason: AuthRejectReason::BadSignature,
        })?;
        mac.update(&claims_json);
        mac.verify_slice(&signature).map_err(|_| AuthReject {
            reason: AuthRejectReason::BadSignature,
        })?;

        let claims: ViewerTokenClaims =
            serde_json::from_slice(&claims_json).map_err(|_| AuthReject {
                reason: AuthRejectReason::MalformedToken,
            })?;
        if claims.audience != VIEWER_AUDIENCE {
            return Err(AuthReject {
                reason: AuthRejectReason::MalformedToken,
            });
        }
        if claims.session_id != session_id {
            return Err(AuthReject {
                reason: AuthRejectReason::WrongSession,
            });
        }
        if claims.viewer_id != viewer_id {
            return Err(AuthReject {
                reason: AuthRejectReason::WrongViewer,
            });
        }
        if claims.origin != origin {
            return Err(AuthReject {
                reason: AuthRejectReason::WrongOrigin,
            });
        }
        if claims.expires_at_ms < now_ms {
            return Err(AuthReject {
                reason: AuthRejectReason::ExpiredToken,
            });
        }
        if let Some(required) = required_capability {
            if !claims.capabilities.iter().any(|cap| cap == required) {
                return Err(AuthReject {
                    reason: AuthRejectReason::CapabilityDenied,
                });
            }
        }
        Ok(claims)
    }
}

pub fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn issuer() -> ViewerTokenIssuer {
        ViewerTokenIssuer::new_for_tests([7; 32])
    }

    #[test]
    fn token_round_trip_binds_session_viewer_origin() {
        let issuer = issuer();
        let token = issuer
            .issue(ViewerTokenClaims {
                audience: VIEWER_AUDIENCE.to_string(),
                session_id: "sess_1".to_string(),
                viewer_id: "view_1".to_string(),
                origin: "http://127.0.0.1:7777".to_string(),
                issued_at_ms: 1000,
                expires_at_ms: 2000,
                capabilities: vec!["view".to_string(), "input".to_string()],
            })
            .expect("token");

        let claims = issuer
            .verify(
                &token,
                "sess_1",
                "view_1",
                "http://127.0.0.1:7777",
                1500,
                Some("input"),
            )
            .expect("valid claims");
        assert_eq!(claims.viewer_id, "view_1");
        assert!(claims.capabilities.iter().any(|cap| cap == "input"));
    }

    #[test]
    fn token_rejects_wrong_origin() {
        let issuer = issuer();
        let token = issuer
            .issue(ViewerTokenClaims {
                audience: VIEWER_AUDIENCE.to_string(),
                session_id: "sess_1".to_string(),
                viewer_id: "view_1".to_string(),
                origin: "http://127.0.0.1:7777".to_string(),
                issued_at_ms: 1000,
                expires_at_ms: 2000,
                capabilities: vec!["view".to_string()],
            })
            .expect("token");

        let err = issuer
            .verify(
                &token,
                "sess_1",
                "view_1",
                "http://localhost:7777",
                1500,
                Some("view"),
            )
            .expect_err("origin rejected");
        assert_eq!(err.reason, AuthRejectReason::WrongOrigin);
    }

    #[test]
    fn token_rejects_expired() {
        let issuer = issuer();
        let token = issuer
            .issue(ViewerTokenClaims {
                audience: VIEWER_AUDIENCE.to_string(),
                session_id: "sess_1".to_string(),
                viewer_id: "view_1".to_string(),
                origin: "http://127.0.0.1:7777".to_string(),
                issued_at_ms: 1000,
                expires_at_ms: 2000,
                capabilities: vec!["view".to_string()],
            })
            .expect("token");

        let err = issuer
            .verify(
                &token,
                "sess_1",
                "view_1",
                "http://127.0.0.1:7777",
                2001,
                Some("view"),
            )
            .expect_err("expired rejected");
        assert_eq!(err.reason, AuthRejectReason::ExpiredToken);
    }

    #[test]
    fn token_rejects_missing_required_capability() {
        let issuer = issuer();
        let token = issuer
            .issue(ViewerTokenClaims {
                audience: VIEWER_AUDIENCE.to_string(),
                session_id: "sess_1".to_string(),
                viewer_id: "view_1".to_string(),
                origin: "http://127.0.0.1:7777".to_string(),
                issued_at_ms: 1000,
                expires_at_ms: 2000,
                capabilities: vec!["view".to_string()],
            })
            .expect("token");

        let err = issuer
            .verify(
                &token,
                "sess_1",
                "view_1",
                "http://127.0.0.1:7777",
                1500,
                Some("input"),
            )
            .expect_err("capability rejected");
        assert_eq!(err.reason, AuthRejectReason::CapabilityDenied);
    }

    #[test]
    fn loopback_host_check_allows_local_addresses() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("[::1]"));
        assert!(!is_loopback_host("example.com"));
    }

    #[test]
    fn default_ttl_is_short() {
        assert_eq!(
            ViewerTokenIssuer::default_ttl(),
            Duration::from_secs(60 * 30)
        );
    }
}
