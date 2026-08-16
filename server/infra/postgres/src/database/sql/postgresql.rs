use std::{str::FromStr, time::Duration};

use sqlx::{
    PgConnection, PgPool, Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TenantDbErr {
    #[error("tenant registration conflicts with existing tenant: {0}")]
    RegistrationConflict(Uuid),
    #[error("tenant is not active: {0}")]
    TenantInactive(Uuid),
    #[error("tenant row-level-security context mismatch: expected {expected}, actual {actual:?}")]
    TenantContextMismatch { expected: Uuid, actual: Option<Uuid> },
    #[error("database role must not be a superuser or have BYPASSRLS")]
    RowLevelSecurityBypassed,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// One shared PostgreSQL pool for global and tenant-owned tables.
///
/// Tenant-owned queries are exposed through `TenantTransaction`, which sets
/// `app.tenant_id` transaction-locally before RLS-protected tables are used.
#[derive(Clone)]
pub struct PostgresCli {
    pool: PgPool,
}

impl PostgresCli {
    pub async fn connect(database_url: &str) -> Result<Self, TenantDbErr> {
        let max_connections: u32 = env_u32("DB_MAX_CONNECTIONS", 15);
        let acquire_timeout: Duration = Duration::from_secs(env_u64("DB_ACQUIRE_TIMEOUT_SECS", 10));
        let connect_timeout: Duration = Duration::from_secs(env_u64("DB_CONNECT_TIMEOUT_SECS", 5));
        let statement_timeout_ms: u64 = env_u64("DB_STATEMENT_TIMEOUT_MS", 15_000);
        let lock_timeout_ms: u64 = env_u64("DB_LOCK_TIMEOUT_MS", 3_000);
        let idle_in_transaction_timeout_ms: u64 = env_u64("DB_IDLE_IN_TRANSACTION_TIMEOUT_MS", 30_000);

        // Shared tables have stable relation OIDs, so SQLx's prepared statement
        // cache can remain enabled. Tenant isolation is provided by row keys
        // and transaction-local RLS context instead of search_path switching.
        let connect_options: PgConnectOptions = PgConnectOptions::from_str(database_url)?;
        let connect = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(acquire_timeout)
            .after_connect(
                move |connection: &mut PgConnection, _metadata: sqlx::pool::PoolConnectionMetadata| {
                    Box::pin(async move {
                        sqlx::query_scalar!(
                            r#"SELECT set_config('statement_timeout', $1, false) AS "set_config!""#,
                            statement_timeout_ms.to_string(),
                        )
                        .fetch_one(&mut *connection)
                        .await?;
                        sqlx::query_scalar!(
                            r#"SELECT set_config('lock_timeout', $1, false) AS "set_config!""#,
                            lock_timeout_ms.to_string(),
                        )
                        .fetch_one(&mut *connection)
                        .await?;
                        sqlx::query_scalar!(
                            r#"SELECT set_config('idle_in_transaction_session_timeout', $1, false) AS "set_config!""#,
                            idle_in_transaction_timeout_ms.to_string(),
                        )
                        .fetch_one(&mut *connection)
                        .await?;
                        Ok(())
                    })
                },
            )
            .connect_with(connect_options);

        let pool: PgPool = tokio::time::timeout(connect_timeout, connect)
            .await
            .map_err(|_| sqlx::Error::PoolTimedOut)??;
        let client = Self { pool };
        client.ensure_rls_capable_role().await?;
        Ok(client)
    }

    async fn ensure_rls_capable_role(&self) -> Result<(), TenantDbErr> {
        let bypasses_rls: bool = sqlx::query_scalar!(
            r#"
            SELECT (rolsuper OR rolbypassrls) AS "bypasses_rls!"
            FROM pg_catalog.pg_roles
            WHERE rolname = CURRENT_USER
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        if bypasses_rls {
            return Err(TenantDbErr::RowLevelSecurityBypassed);
        }
        Ok(())
    }

    pub async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, TenantDbErr> {
        let mut transaction: Transaction<'static, Postgres> = self.pool.begin().await?;
        let tenant_is_active: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM tenants WHERE id = $1 AND status = 'active') AS "exists!""#,
            tenant_id,
        )
        .fetch_one(&mut *transaction)
        .await?;
        if !tenant_is_active {
            return Err(TenantDbErr::TenantInactive(tenant_id));
        }

        // `true` makes the setting LOCAL to this transaction. Commit, rollback,
        // or drop therefore cannot leak a tenant context through the pool.
        sqlx::query_scalar!(
            r#"SELECT set_config('app.tenant_id', $1, true) AS "tenant_context!""#,
            tenant_id.to_string(),
        )
        .fetch_one(&mut *transaction)
        .await?;
        let effective_tenant_id: Option<Uuid> =
            sqlx::query_scalar!(r#"SELECT NULLIF(current_setting('app.tenant_id', true), '')::UUID AS "tenant_id?""#,)
                .fetch_one(&mut *transaction)
                .await?;
        if effective_tenant_id != Some(tenant_id) {
            return Err(TenantDbErr::TenantContextMismatch {
                expected: tenant_id,
                actual: effective_tenant_id,
            });
        }

        Ok(TenantTransaction { tenant_id, transaction })
    }

    pub(crate) async fn resolve_active_tenant_id(&self, tenant: &str) -> Result<Option<Uuid>, TenantDbErr> {
        let tenant_id: Option<Uuid> = sqlx::query_scalar!(
            r#"SELECT id FROM tenants WHERE slug = $1 AND status = 'active'"#,
            tenant,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(tenant_id)
    }

    pub(crate) async fn ensure_tenant_registration(
        &self,
        tenant_id: Uuid,
        slug: &str,
        display_name: &str,
    ) -> Result<(), TenantDbErr> {
        let existing = sqlx::query!(
            r#"SELECT slug AS "slug!", status AS "status!" FROM tenants WHERE id = $1"#,
            tenant_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(existing) = existing {
            if existing.slug != slug {
                return Err(TenantDbErr::RegistrationConflict(tenant_id));
            }
            if existing.status != "active" {
                return Err(TenantDbErr::TenantInactive(tenant_id));
            }
            sqlx::query!(
                "UPDATE tenants SET display_name = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
                tenant_id,
                display_name,
            )
            .execute(&self.pool)
            .await?;
            return Ok(());
        }

        let inserted = sqlx::query!(
            r#"
            INSERT INTO tenants (id, slug, display_name, status)
            VALUES ($1, $2, $3, 'active')
            "#,
            tenant_id,
            slug,
            display_name,
        )
        .execute(&self.pool)
        .await;
        match inserted {
            Ok(_) => Ok(()),
            Err(error)
                if error
                    .as_database_error()
                    .is_some_and(|database_error| database_error.is_unique_violation()) =>
            {
                Err(TenantDbErr::RegistrationConflict(tenant_id))
            }
            Err(error) => Err(TenantDbErr::Sqlx(error)),
        }
    }

    /// Access the native SQLx pool for application-owned queries.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ready(&self) -> bool {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }
}

pub struct TenantTransaction {
    tenant_id: Uuid,
    transaction: Transaction<'static, Postgres>,
}

impl TenantTransaction {
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    pub fn connection(&mut self) -> &mut PgConnection {
        &mut self.transaction
    }

    pub async fn commit(self) -> Result<(), sqlx::Error> {
        self.transaction.commit().await
    }

    pub async fn rollback(self) -> Result<(), sqlx::Error> {
        self.transaction.rollback().await
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value: String| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value: String| value.parse::<u32>().ok())
        .filter(|value: &u32| *value > 0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rls_hides_and_rejects_cross_tenant_accounts() -> Result<(), Box<dyn std::error::Error>> {
        let database_url: String = std::env::var("DATABASE_URL")?;
        let client: PostgresCli = PostgresCli::connect(&database_url).await?;
        let bypasses_rls: bool = sqlx::query_scalar!(
            r#"
            SELECT (rolsuper OR rolbypassrls) AS "bypasses_rls!"
            FROM pg_catalog.pg_roles
            WHERE rolname = CURRENT_USER
            "#,
        )
        .fetch_one(client.pool())
        .await?;
        assert!(!bypasses_rls, "the test database role must be subject to RLS");

        let tenant_a: Uuid = Uuid::new_v4();
        let tenant_b: Uuid = Uuid::new_v4();
        let account_a: Uuid = Uuid::new_v4();
        let tenant_a_slug: String = format!("rls-a-{}", tenant_a.simple());
        let tenant_b_slug: String = format!("rls-b-{}", tenant_b.simple());
        client
            .ensure_tenant_registration(tenant_a, &tenant_a_slug, "RLS tenant A")
            .await?;
        client
            .ensure_tenant_registration(tenant_b, &tenant_b_slug, "RLS tenant B")
            .await?;

        let mut tenant_a_transaction: TenantTransaction = client.begin_tenant(tenant_a).await?;
        sqlx::query!(
            r#"
            INSERT INTO accounts (id, tenant_id, username, primary_role_code)
            VALUES ($1, $2, 'rls-test-user', 'employee')
            "#,
            account_a,
            tenant_a,
        )
        .execute(tenant_a_transaction.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO account_roles (tenant_id, account_id, role_code)
            VALUES ($1, $2, 'employee')
            "#,
            tenant_a,
            account_a,
        )
        .execute(tenant_a_transaction.connection())
        .await?;
        tenant_a_transaction.commit().await?;

        let mut tenant_b_transaction: TenantTransaction = client.begin_tenant(tenant_b).await?;
        let visible_account_count: i64 = sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM accounts"#)
            .fetch_one(tenant_b_transaction.connection())
            .await?;
        assert_eq!(visible_account_count, 0, "RLS leaked another tenant's account");

        let cross_tenant_insert: Result<sqlx::postgres::PgQueryResult, sqlx::Error> = sqlx::query!(
            r#"
            INSERT INTO accounts (id, tenant_id, username, primary_role_code)
            VALUES ($1, $2, 'rls-cross-tenant', 'employee')
            "#,
            Uuid::new_v4(),
            tenant_a,
        )
        .execute(tenant_b_transaction.connection())
        .await;
        assert!(cross_tenant_insert.is_err(), "RLS allowed a row for a different tenant");
        tenant_b_transaction.rollback().await?;

        let mut cleanup_transaction: TenantTransaction = client.begin_tenant(tenant_a).await?;
        sqlx::query!("DELETE FROM accounts WHERE tenant_id = $1", tenant_a)
            .execute(cleanup_transaction.connection())
            .await?;
        cleanup_transaction.commit().await?;
        sqlx::query!("DELETE FROM tenants WHERE id = ANY($1)", &[tenant_a, tenant_b])
            .execute(client.pool())
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn attendance_allows_multiple_completed_sessions_but_one_open_session()
    -> Result<(), Box<dyn std::error::Error>> {
        let database_url: String = std::env::var("DATABASE_URL")?;
        let client: PostgresCli = PostgresCli::connect(&database_url).await?;
        let tenant_id: Uuid = Uuid::new_v4();
        let account_id: Uuid = Uuid::new_v4();
        let employee_id: Uuid = Uuid::new_v4();
        let branch_id: Uuid = Uuid::new_v4();
        let facility_id: Uuid = Uuid::new_v4();
        let first_session_id: Uuid = Uuid::new_v4();
        let tenant_slug: String = format!("attendance-{}", tenant_id.simple());
        client
            .ensure_tenant_registration(tenant_id, &tenant_slug, "Attendance test tenant")
            .await?;

        let mut transaction: TenantTransaction = client.begin_tenant(tenant_id).await?;
        sqlx::query!(
            r#"
            INSERT INTO accounts (id, tenant_id, username, primary_role_code)
            VALUES ($1, $2, 'attendance-test-user', 'employee')
            "#,
            account_id,
            tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO branches (id, tenant_id, code, name)
            VALUES ($1, $2, 'attendance-test-branch', 'Attendance test branch')
            "#,
            branch_id,
            tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO facilities (id, tenant_id, branch_id, code, name)
            VALUES ($1, $2, $3, 'attendance-test-facility', 'Attendance test facility')
            "#,
            facility_id,
            tenant_id,
            branch_id,
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO account_roles (tenant_id, account_id, role_code)
            VALUES ($1, $2, 'employee')
            "#,
            tenant_id,
            account_id,
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, account_id, employee_code, display_name, status, hire_date
            )
            VALUES ($1, $2, $3, 'attendance-test-employee', 'Attendance test employee', 'active', CURRENT_DATE)
            "#,
            employee_id,
            tenant_id,
            account_id,
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO hr_attendance_sessions (
                id, tenant_id, employee_id, facility_id, check_in_at, check_in_by_account_id
            )
            VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP - INTERVAL '2 hours', $5)
            "#,
            first_session_id,
            tenant_id,
            employee_id,
            facility_id,
            account_id,
        )
        .execute(transaction.connection())
        .await?;

        sqlx::query!("SAVEPOINT duplicate_open_attendance_session")
            .execute(transaction.connection())
            .await?;
        let duplicate_open_session: Result<sqlx::postgres::PgQueryResult, sqlx::Error> = sqlx::query!(
            r#"
            INSERT INTO hr_attendance_sessions (id, tenant_id, employee_id, facility_id, check_in_by_account_id)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            Uuid::new_v4(),
            tenant_id,
            employee_id,
            facility_id,
            account_id,
        )
        .execute(transaction.connection())
        .await;
        assert!(
            duplicate_open_session.is_err(),
            "an employee must have at most one open attendance session"
        );
        sqlx::query!("ROLLBACK TO SAVEPOINT duplicate_open_attendance_session")
            .execute(transaction.connection())
            .await?;

        let worked_seconds: i64 = sqlx::query_scalar!(
            r#"
            UPDATE hr_attendance_sessions
            SET check_out_at = CURRENT_TIMESTAMP - INTERVAL '1 hour',
                check_out_by_account_id = $3,
                updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2
            RETURNING worked_seconds AS "worked_seconds!"
            "#,
            tenant_id,
            first_session_id,
            account_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        assert!(
            worked_seconds >= 3_599,
            "completed attendance must retain its computed duration"
        );

        sqlx::query!(
            r#"
            INSERT INTO hr_attendance_sessions (id, tenant_id, employee_id, facility_id, check_in_by_account_id)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            Uuid::new_v4(),
            tenant_id,
            employee_id,
            facility_id,
            account_id,
        )
        .execute(transaction.connection())
        .await?;
        let session_count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM hr_attendance_sessions
            WHERE tenant_id = $1 AND employee_id = $2
            "#,
            tenant_id,
            employee_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        assert_eq!(session_count, 2, "a completed session must not block the next check in");
        let sessions_at_facility: i64 = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM hr_attendance_sessions
            WHERE tenant_id = $1 AND employee_id = $2 AND facility_id = $3
            "#,
            tenant_id,
            employee_id,
            facility_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        assert_eq!(
            sessions_at_facility, session_count,
            "every attendance session must preserve its work facility"
        );
        transaction.rollback().await?;

        sqlx::query!("DELETE FROM tenants WHERE id = $1", tenant_id)
            .execute(client.pool())
            .await?;
        Ok(())
    }
}
