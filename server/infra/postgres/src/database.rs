use std::sync::Arc;

use infra_kernel::debug::*;
use uuid::Uuid;
use sqlx::{
    PgConnection, PgPool, Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};

pub mod sql;

pub use sql::postgresql::{PostgresCli, TenantDbErr, TenantTransaction};

pub struct DatabaseAdapter {
    client: PostgresCli,
}

impl DatabaseAdapter {
    pub async fn new_arc() -> Arc<Self> {
        log_notice!("PostgreSQL adapter initialization started");
        let database_url: String = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| panic!("DATABASE_URL must be configured before database initialization"));
        log_debug!("DATABASE_URL is configured; credentials and URL are intentionally not logged");
        log_info!("Opening PostgreSQL shared-table connection pool");
        Self::connect(&database_url).await.unwrap_or_else(|error| {
            log_error!("Failed to connect to PostgreSQL: {}", error);
            panic!("Failed to connect to PostgreSQL");
        })
    }

    pub async fn connect(database_url: &str) -> Result<Arc<Self>, TenantDbErr> {
        let client: PostgresCli = PostgresCli::connect(database_url).await?;
        log_notice!("PostgreSQL shared-table connection pool initialized");
        Ok(Arc::new(Self { client }))
    }

    pub fn client(&self) -> &PostgresCli {
        &self.client
    }

    pub fn pool(&self) -> &sqlx::PgPool {
        self.client.pool()
    }

    pub async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, TenantDbErr> {
        log_debug!("Opening RLS-scoped tenant transaction: tenant_id={}", tenant_id);
        self.client.begin_tenant(tenant_id).await
    }

    pub async fn resolve_active_tenant_id(&self, tenant: &str) -> Result<Option<Uuid>, TenantDbErr> {
        log_debug!("Resolving active tenant ID: tenant={}", tenant);
        let result: Result<Option<Uuid>, TenantDbErr> = self.client.resolve_active_tenant_id(tenant).await;
        match &result {
            Ok(Some(tenant_id)) => {
                log_debug!("Active tenant ID resolved: tenant={} tenant_id={}", tenant, tenant_id)
            }
            Ok(None) => log_info!("No active tenant ID found: tenant={}", tenant),
            Err(error) => log_error!("Active tenant ID resolution failed: tenant={} error={}", tenant, error),
        }
        result
    }

    /// Idempotently registers one active tenant in the shared tenant table.
    pub async fn provision_tenant(&self, tenant_id: Uuid, slug: &str, display_name: &str) -> Result<(), TenantDbErr> {
        log_notice!(
            "Tenant provisioning started: tenant_id={} slug={} display_name={}",
            tenant_id,
            slug,
            display_name
        );
        self.client
            .ensure_tenant_registration(tenant_id, slug, display_name)
            .await?;
        log_notice!(
            "Tenant provisioning completed: tenant_id={} slug={} status=active",
            tenant_id,
            slug
        );
        Ok(())
    }
}

impl DatabaseAdapter {
    pub async fn ready(&self) -> bool {
        self.client.ready().await
    }
}
