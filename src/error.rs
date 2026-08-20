//! Public error contract. Internal errors must be converted here before crossing MCP.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    PolicyDenied,
    RateLimited,
    BackendUnavailable,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicError {
    pub code: ErrorCode,
    pub message: &'static str,
    pub retryable: bool,
}

impl PublicError {
    pub const fn invalid_request() -> Self {
        Self {
            code: ErrorCode::InvalidRequest,
            message: "the request is invalid",
            retryable: false,
        }
    }

    pub const fn policy_denied() -> Self {
        Self {
            code: ErrorCode::PolicyDenied,
            message: "the request is not allowed by policy",
            retryable: false,
        }
    }

    pub const fn backend_unavailable() -> Self {
        Self {
            code: ErrorCode::BackendUnavailable,
            message: "the database backend is unavailable",
            retryable: true,
        }
    }

    pub const fn rate_limited() -> Self {
        Self {
            code: ErrorCode::RateLimited,
            message: "request rate limit exceeded",
            retryable: true,
        }
    }

    pub const fn internal() -> Self {
        Self {
            code: ErrorCode::InternalError,
            message: "the operation could not be completed",
            retryable: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_stable_sanitized_policy_error() {
        let error = PublicError::policy_denied();
        let json = serde_json::to_string(&error).expect("serializable error");
        assert_eq!(
            json,
            r#"{"code":"policy_denied","message":"the request is not allowed by policy","retryable":false}"#
        );
        assert!(!json.contains("SELECT"));
        assert!(!json.contains("password"));
        assert!(!json.contains("users"));
    }

    #[test]
    fn only_backend_errors_are_retryable() {
        assert!(!PublicError::invalid_request().retryable);
        assert!(!PublicError::policy_denied().retryable);
        assert!(PublicError::backend_unavailable().retryable);
        assert!(!PublicError::internal().retryable);
    }
}
