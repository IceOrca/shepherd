use std::{future::Future, sync::Arc};

use sqlx::PgConnection;
use tracing::{error, warn, info, debug, trace};
use uuid::Uuid;
pub mod postgresql;
pub use postgresql::{PostgresCli, TenantDbErr, TenantTransaction};

tokio::task_local! {
    static ACTIVE_BRANCH_ID: Uuid;
}

pub async fn with_active_branch<T, F>(branch_id: Uuid, future: F) -> T
where
    F: Future<Output = T>,
{
    trace!(
        operation = "postgres.with_active_branch",
        branch_id = %branch_id,
        "Entering request-scoped active branch context"
    );
    ACTIVE_BRANCH_ID.scope(branch_id, future).await
}

pub fn active_branch_id() -> Option<Uuid> {
    ACTIVE_BRANCH_ID.try_with(|branch_id: &Uuid| *branch_id).ok()
}

pub struct DatabaseAdapter {
    client: PostgresCli,
}

impl DatabaseAdapter {
    pub async fn new_arc() -> Arc<Self> {
        info!("PostgreSQL adapter initialization started");
        let database_url: String = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| panic!("DATABASE_URL must be configured before database initialization"));
        debug!("DATABASE_URL is configured; credentials and URL are intentionally not logged");
        info!("Opening PostgreSQL shared-table connection pool");
        Self::connect(&database_url).await.unwrap_or_else(|error: TenantDbErr| {
            panic!("Failed to connect to PostgreSQL: {}", error);
        })
    }

    pub async fn connect(database_url: &str) -> Result<Arc<Self>, TenantDbErr> {
        let client: PostgresCli = PostgresCli::connect(database_url).await?;
        info!("PostgreSQL shared-table connection pool initialized");
        Ok(Arc::new(Self { client }))
    }

    pub fn client(&self) -> &PostgresCli {
        &self.client
    }

    /// Exposes the unscoped connection pool only for tables that intentionally
    /// exist outside tenant RLS, such as tenant discovery and identity lookup.
    /// Tenant-owned application data must use `tran_with_tenant` or
    /// `begin_tenant`.
    pub fn global_pool(&self) -> &sqlx::PgPool {
        self.client.pool()
    }

    pub async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, TenantDbErr> {
        let branch_id: Option<Uuid> = active_branch_id();
        trace!(
            operation = "postgres.begin_tenant",
            tenant_id = %tenant_id,
            branch_id = ?branch_id,
            "Opening RLS-scoped tenant transaction"
        );
        self.client.begin_tenant_with_branch(tenant_id, branch_id).await
    }

    /// Runs one SQLx operation inside an automatically committed tenant- and
    /// active-branch-scoped transaction. Multi-step domain workflows coordinate locks or map
    /// business errors should continue to use `begin_tenant` explicitly.
    pub async fn tran_with_tenant<T, F>(&self, tenant_id: Uuid, op: F) -> Result<T, TenantDbErr>
    where
        T: Send,
        F: for<'conn> AsyncFnOnce(&'conn mut PgConnection) -> Result<T, sqlx::Error>,
    {
        let branch_id: Option<Uuid> = active_branch_id();
        self.client.tran_with_tenant_and_branch(tenant_id, branch_id, op).await
    }

    pub async fn resolve_active_tenant_id(&self, tenant: &str) -> Result<Option<Uuid>, TenantDbErr> {
        trace!("Resolving active tenant ID: tenant={}", tenant);
        let result: Result<Option<Uuid>, TenantDbErr> = self.client.resolve_active_tenant_id(tenant).await;
        match &result {
            Ok(Some(tenant_id)) => {
                debug!("Active tenant ID resolved: tenant={} tenant_id={}", tenant, tenant_id)
            }
            Ok(None) => info!("No active tenant ID found: tenant={}", tenant),
            Err(error) => error!("Active tenant ID resolution failed: tenant={} error={}", tenant, error),
        }
        result
    }

    /// Idempotently registers one active tenant in the shared tenant table.
    pub async fn provision_tenant(&self, tenant_id: Uuid, slug: &str, display_name: &str) -> Result<(), TenantDbErr> {
        info!(
            "Tenant provisioning started: tenant_id={} slug={} display_name={}",
            tenant_id, slug, display_name
        );
        self.client
            .ensure_tenant_registration(tenant_id, slug, display_name)
            .await?;
        info!(
            "Tenant provisioning completed: tenant_id={} slug={} status=active",
            tenant_id, slug
        );
        Ok(())
    }
}

impl DatabaseAdapter {
    pub async fn ready(&self) -> bool {
        self.client.ready().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn one_shot_transaction_propagates_active_branch_context() -> Result<(), Box<dyn std::error::Error>> {
        let database_url: String = std::env::var("DATABASE_URL")?;
        let database: Arc<DatabaseAdapter> = DatabaseAdapter::connect(&database_url).await?;
        let tenant_id: Uuid = Uuid::new_v4();
        let branch_id: Uuid = Uuid::new_v4();
        let tenant_slug: String = format!("branch-context-{}", tenant_id.simple());
        database
            .provision_tenant(tenant_id, &tenant_slug, "Branch context test")
            .await?;

        let observed_branch_id: Option<Uuid> = with_active_branch(
            branch_id,
            database.tran_with_tenant(tenant_id, async |connection: &mut PgConnection| {
                sqlx::query_scalar!(
                    r#"SELECT NULLIF(current_setting('app.branch_id', TRUE), '')::UUID AS "branch_id?""#,
                )
                .fetch_one(connection)
                .await
            }),
        )
        .await?;
        assert_eq!(observed_branch_id, Some(branch_id));

        sqlx::query!("DELETE FROM tenants WHERE id = $1", tenant_id)
            .execute(database.global_pool())
            .await?;
        Ok(())
    }
}
