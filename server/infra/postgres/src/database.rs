use std::sync::Arc;

use sqlx::PgConnection;
use tracing::{debug, error, info};
use uuid::Uuid;

pub mod sql;

pub use sql::postgresql::{PostgresCli, TenantDbErr, TenantTransaction};

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
        Self::connect(&database_url).await.unwrap_or_else(|error| {
            error!("Failed to connect to PostgreSQL: {}", error);
            panic!("Failed to connect to PostgreSQL");
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
    /// Tenant-owned application data must use `run_with_tenant` or
    /// `begin_tenant`.
    pub fn global_pool(&self) -> &sqlx::PgPool {
        self.client.pool()
    }

    pub async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, TenantDbErr> {
        debug!("Opening RLS-scoped tenant transaction: tenant_id={}", tenant_id);
        self.client.begin_tenant(tenant_id).await
    }

    /// Runs one SQLx operation inside an automatically committed tenant-scoped
    /// transaction. Multi-step domain workflows that coordinate locks or map
    /// business errors should continue to use `begin_tenant` explicitly.
    pub async fn run_with_tenant<T, F>(&self, tenant_id: Uuid, operation: F) -> Result<T, TenantDbErr>
    where
        T: Send,
        F: for<'connection> AsyncFnOnce(&'connection mut PgConnection) -> Result<T, sqlx::Error>,
    {
        self.client.run_with_tenant(tenant_id, operation).await
    }

    pub async fn resolve_active_tenant_id(&self, tenant: &str) -> Result<Option<Uuid>, TenantDbErr> {
        debug!("Resolving active tenant ID: tenant={}", tenant);
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
