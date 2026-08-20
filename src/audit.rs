//! Structured audit events for every accepted or rejected gateway operation.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("unable to serialize audit event")]
    Serialize(#[source] serde_json::Error),
    #[error("unable to write audit event to `{path}`")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Accepted,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEvent {
    pub request_id: String,
    pub operation: String,
    pub outcome: AuditOutcome,
    pub object_kind: Option<String>,
    pub object_name: Option<String>,
    pub row_limit: Option<u32>,
    pub duration_ms: u64,
}

#[allow(dead_code)]
impl AuditEvent {
    pub fn new(request_id: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            operation: operation.into(),
            outcome: AuditOutcome::Accepted,
            object_kind: None,
            object_name: None,
            row_limit: None,
            duration_ms: 0,
        }
    }

    pub fn elapsed(mut self, duration: Duration) -> Self {
        self.duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        self
    }

    pub fn outcome(mut self, outcome: AuditOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    pub fn object(mut self, kind: impl Into<String>, name: impl Into<String>) -> Self {
        self.object_kind = Some(kind.into());
        self.object_name = Some(name.into());
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.row_limit = Some(limit);
        self
    }
}

#[derive(Debug, Clone)]
pub struct AuditLogger {
    path: PathBuf,
}

impl AuditLogger {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Appends one JSON object per line; SQL and request values are never accepted here.
    pub async fn record(&self, event: &AuditEvent) -> Result<(), AuditError> {
        use tokio::io::AsyncWriteExt;

        let mut line = serde_json::to_vec(event).map_err(AuditError::Serialize)?;
        line.push(b'\n');
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .map_err(|source| AuditError::Write {
                path: self.path.clone(),
                source,
            })?;
        file.write_all(&line)
            .await
            .map_err(|source| AuditError::Write {
                path: self.path.clone(),
                source,
            })?;
        file.flush().await.map_err(|source| AuditError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn test_path() -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("copilot-datagate-audit-{unique}"))
    }

    #[tokio::test]
    async fn writes_one_structured_event_per_line() {
        let path = test_path();
        let _ = tokio::fs::remove_file(&path).await;
        let logger = AuditLogger::new(&path);
        let event = AuditEvent::new("request-1", "select")
            .object("table", "users")
            .limit(10)
            .elapsed(Duration::from_millis(7));

        logger.record(&event).await.expect("audit event written");
        let content = tokio::fs::read_to_string(&path).await.expect("audit file");
        let parsed: AuditEvent = serde_json::from_str(content.trim()).expect("valid event");
        assert_eq!(parsed, event);
        assert!(content.ends_with('\n'));
        let _ = tokio::fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn records_rejection_without_query_or_values() {
        let path = test_path();
        let _ = tokio::fs::remove_file(&path).await;
        let logger = AuditLogger::new(&path);
        let event = AuditEvent::new("request-2", "select").outcome(AuditOutcome::Rejected);

        logger.record(&event).await.expect("audit event written");
        let content = tokio::fs::read_to_string(&path).await.expect("audit file");
        assert!(!content.contains("SELECT"));
        assert!(!content.contains("password"));
        assert!(content.contains("rejected"));
        let _ = tokio::fs::remove_file(path).await;
    }
}
