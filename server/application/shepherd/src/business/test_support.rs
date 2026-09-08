//! Rollback-only business fixtures: even a failing assertion cannot leave data.
use std::{error::Error, sync::Arc};

use infra_postgres::DatabaseAdapter;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) struct Fixture {
    pub transaction: Transaction<'static, Postgres>,
    pub tenant_id: Uuid,
    pub branch_id: Uuid,
    pub sibling_id: Uuid,
    pub staff_account_id: Uuid,
    pub manager_account_id: Uuid,
    pub staff_id: Uuid,
    pub manager_id: Uuid,
}

impl Fixture {
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        let database_url: String = std::env::var("DATABASE_URL")?;
        let database: Arc<DatabaseAdapter> = DatabaseAdapter::connect(&database_url).await?;
        // Raw pool access is confined to registering this uncommitted test tenant.
        let mut transaction: Transaction<'static, Postgres> = database.pool().begin().await?;
        let tenant_id: Uuid = Uuid::new_v4();
        let branch_id: Uuid = Uuid::new_v4();
        let sibling_id: Uuid = Uuid::new_v4();
        let staff_account_id: Uuid = Uuid::new_v4();
        let manager_account_id: Uuid = Uuid::new_v4();
        let staff_id: Uuid = Uuid::new_v4();
        let manager_id: Uuid = Uuid::new_v4();
        sqlx::query!(
            "SELECT set_config('app.tenant_id', $1, TRUE) AS tenant_context, set_config('app.branch_id', $2, TRUE) AS branch_context",
            tenant_id.to_string(), branch_id.to_string(),
        ).fetch_one(&mut *transaction).await?;
        sqlx::query!(
            "INSERT INTO tenants (id, slug, display_name) VALUES ($1, $2, 'Rollback business regression')",
            tenant_id,
            format!("test-business-regression-{}", tenant_id.simple()),
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query!(
            "INSERT INTO branches (id, tenant_id, code, name, time_zone) VALUES ($1, $3, 'test-main', 'Main', 'Asia/Bangkok'), ($2, $3, 'test-sibling', 'Sibling', 'Asia/Bangkok')",
            branch_id, sibling_id, tenant_id,
        ).execute(&mut *transaction).await?;
        sqlx::query!(
            "INSERT INTO accounts (id, tenant_id, username, primary_role_code) VALUES ($1, $3, 'test-staff', 'staff'), ($2, $3, 'test-manager', 'branch_manager')",
            staff_account_id, manager_account_id, tenant_id,
        ).execute(&mut *transaction).await?;
        sqlx::query!(
            "INSERT INTO account_role_assignments (tenant_id, account_id, role_code, branch_id) VALUES ($1, $2, 'staff', $4), ($1, $3, 'branch_manager', $4)",
            tenant_id, staff_account_id, manager_account_id, branch_id,
        ).execute(&mut *transaction).await?;
        sqlx::query!(
            "INSERT INTO hr_employees (id, tenant_id, branch_id, account_id, employee_code, display_name, status, hire_date) VALUES ($1, $3, $4, $5, 'test-staff', 'Staff', 'active', DATE '2026-01-01'), ($2, $3, $4, $6, 'test-manager', 'Original manager', 'active', DATE '2026-01-01')",
            staff_id, manager_id, tenant_id, branch_id, staff_account_id, manager_account_id,
        ).execute(&mut *transaction).await?;
        Ok(Self {
            transaction,
            tenant_id,
            branch_id,
            sibling_id,
            staff_account_id,
            manager_account_id,
            staff_id,
            manager_id,
        })
    }
}
