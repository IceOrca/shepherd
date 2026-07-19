use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub enum AuditOutcome {
    Accepted,
    Rejected,
    Failed,
}

impl AuditOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }
}

/// Emit a structured security or administration audit event.
///
/// Callers should log identifiers and action codes only; credentials, tokens,
/// salary values, and other sensitive payloads do not belong in audit logs.
pub fn record(action: &str, outcome: AuditOutcome, tenant_id: Option<Uuid>, actor_id: Option<Uuid>) {
    tracing::info!(
        audit = true,
        action,
        outcome = outcome.as_str(),
        tenant_id = ?tenant_id,
        actor_id = ?actor_id,
        "audit event"
    );
}
