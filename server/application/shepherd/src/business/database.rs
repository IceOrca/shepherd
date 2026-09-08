use sqlx::PgConnection;
use uuid::Uuid;

/// Financial writers take the branch barrier before any employee/subject lock.
/// Period close uses the same order; reads do not need this serialization.
pub(crate) async fn lock_active_branch(connection: &mut PgConnection, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    let branch_id: Uuid = sqlx::query_scalar!(
        "SELECT id FROM branches WHERE tenant_id = $1 AND id = shepherd_current_branch_id() FOR UPDATE",
        tenant_id,
    )
    .fetch_one(connection)
    .await?;
    tracing::trace!(%tenant_id, %branch_id, "Business branch mutation lock acquired");
    Ok(())
}
