#![cfg_attr(debug_assertions, allow(unused))]

use std::{collections::HashMap, error::Error, io, sync::Arc};

use chrono::{NaiveDate, NaiveTime};
use tracing::{error, warn, info, debug, trace};
use infra_kernel::debug::Debugging;
use infra_postgres::DatabaseAdapter;
use uuid::Uuid;

const ACME_TENANT_ID: &str = "00000000-0000-4000-8000-000000000001";
const ACME1_TENANT_ID: &str = "00000000-0000-4000-8000-000000000002";
const ACME2_TENANT_ID: &str = "00000000-0000-4000-8000-000000000003";
const DEV_ATTENDANCE_ID_NAMESPACE: u128 = 0xd3a7_7e00_0000_4000_8000_0000_0000_0000;
const DEV_STAFFING_SHIFT_ID_NAMESPACE: u128 = 0x51f7_0000_0000_4000_8000_0000_0000_0000;
const DEV_STAFFING_ASSIGNMENT_ID_NAMESPACE: u128 = 0xa551_0000_0000_4000_8000_0000_0000_0000;
const DEV_URGENT_BATCH_ID_NAMESPACE: u128 = 0xb47c_0000_0000_4000_8000_0000_0000_0000;
const DEV_URGENT_REPORT_ID_NAMESPACE: u128 = 0xc47c_0000_0000_4000_8000_0000_0000_0000;
const DEV_URGENT_SESSION_ID_NAMESPACE: u128 = 0xd47c_0000_0000_4000_8000_0000_0000_0000;
const DEV_URGENT_CUSTOMER_RECORD_ID_NAMESPACE: u128 = 0xe47c_0000_0000_4000_8000_0000_0000_0000;

struct SeedIdRow {
    id: Uuid,
}

struct ExistingDevAccountRow {
    id: Uuid,
    email: Option<String>,
    primary_role_code: String,
}

struct DevTenant {
    id: &'static str,
    slug: &'static str,
    display_name: &'static str,
    accounts: &'static [DevAccount],
}

struct DevBranch {
    code: &'static str,
    name: &'static str,
    time_zone: &'static str,
    facilities: &'static [DevFacility],
}

struct DevFacility {
    code: &'static str,
    name: &'static str,
}

struct DevAccount {
    username: &'static str,
    role: DevRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DevRole {
    Owner,
    Manager,
    Staff,
}

impl DevRole {
    const fn as_code(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Manager => "manager",
            Self::Staff => "staff",
        }
    }
}

fn dev_auth_email(tenant_slug: &str, username: &str) -> String {
    if username == "iceorca" {
        return "iceorca@shepherd.local".to_owned();
    }
    let local_suffix: String = username
        .strip_prefix(&format!("{tenant_slug}_"))
        .unwrap_or(username)
        .replace('_', "");
    format!("{tenant_slug}.{local_suffix}@shepherd.local")
}

#[derive(Clone, Debug)]
struct SeedAccount {
    id: Uuid,
    username: String,
    role: DevRole,
}

const HEAD_OFFICE_FACILITIES: &[DevFacility] = &[
    DevFacility {
        code: "main-office",
        name: "Main Office",
    },
    DevFacility {
        code: "warehouse",
        name: "Warehouse",
    },
];

const NORTH_BRANCH_FACILITIES: &[DevFacility] = &[
    DevFacility {
        code: "office",
        name: "Branch Office",
    },
    DevFacility {
        code: "warehouse",
        name: "Branch Warehouse",
    },
];

const DEV_BRANCHES: &[DevBranch] = &[
    DevBranch {
        code: "head-office",
        name: "Head Office",
        time_zone: "Asia/Bangkok",
        facilities: HEAD_OFFICE_FACILITIES,
    },
    DevBranch {
        code: "north-branch",
        name: "North Branch",
        time_zone: "Asia/Bangkok",
        facilities: NORTH_BRANCH_FACILITIES,
    },
];

const ACME_ACCOUNTS: &[DevAccount] = &[
    DevAccount {
        username: "iceorca",
        role: DevRole::Owner,
    },
    DevAccount {
        username: "acme_manager_1",
        role: DevRole::Manager,
    },
    DevAccount {
        username: "acme_manager_2",
        role: DevRole::Manager,
    },
    DevAccount {
        username: "acme_staff_1",
        role: DevRole::Staff,
    },
    DevAccount {
        username: "acme_staff_2",
        role: DevRole::Staff,
    },
    DevAccount {
        username: "acme_staff_3",
        role: DevRole::Staff,
    },
    DevAccount {
        username: "acme_staff_4",
        role: DevRole::Staff,
    },
];

const ACME1_ACCOUNTS: &[DevAccount] = &[
    DevAccount {
        username: "acme1_owner",
        role: DevRole::Owner,
    },
    DevAccount {
        username: "acme1_manager_1",
        role: DevRole::Manager,
    },
    DevAccount {
        username: "acme1_manager_2",
        role: DevRole::Manager,
    },
    DevAccount {
        username: "acme1_staff_1",
        role: DevRole::Staff,
    },
    DevAccount {
        username: "acme1_staff_2",
        role: DevRole::Staff,
    },
    DevAccount {
        username: "acme1_staff_3",
        role: DevRole::Staff,
    },
    DevAccount {
        username: "acme1_staff_4",
        role: DevRole::Staff,
    },
];

const ACME2_ACCOUNTS: &[DevAccount] = &[
    DevAccount {
        username: "acme2_owner",
        role: DevRole::Owner,
    },
    DevAccount {
        username: "acme2_manager_1",
        role: DevRole::Manager,
    },
    DevAccount {
        username: "acme2_manager_2",
        role: DevRole::Manager,
    },
    DevAccount {
        username: "acme2_staff_1",
        role: DevRole::Staff,
    },
    DevAccount {
        username: "acme2_staff_2",
        role: DevRole::Staff,
    },
    DevAccount {
        username: "acme2_staff_3",
        role: DevRole::Staff,
    },
    DevAccount {
        username: "acme2_staff_4",
        role: DevRole::Staff,
    },
];

const DEV_TENANTS: &[DevTenant] = &[
    DevTenant {
        id: ACME_TENANT_ID,
        slug: "acme",
        display_name: "Acme Corporation",
        accounts: ACME_ACCOUNTS,
    },
    DevTenant {
        id: ACME1_TENANT_ID,
        slug: "acme1",
        display_name: "Acme 1 Corporation",
        accounts: ACME1_ACCOUNTS,
    },
    DevTenant {
        id: ACME2_TENANT_ID,
        slug: "acme2",
        display_name: "Acme 2 Corporation",
        accounts: ACME2_ACCOUNTS,
    },
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Debugging::init();
    info!("Development seed started: configured_tenants={}", DEV_TENANTS.len());
    require_development_environment()?;
    let auth_issuer: String = std::env::var("AUTH_ISSUER_URL")
        .map_err(|_| io::Error::other("AUTH_ISSUER_URL is required for development seeding"))?;
    let auth_identities_json: String = std::env::var("AUTH_DEV_IDENTITIES_JSON")
        .map_err(|_| io::Error::other("AUTH_DEV_IDENTITIES_JSON is required for development seeding"))?;
    let auth_identities: HashMap<String, Uuid> = serde_json::from_str(&auth_identities_json)
        .map_err(|error: serde_json::Error| io::Error::other(format!("parse Auth identity map: {error}")))?;
    let expected_account_count: usize = DEV_TENANTS
        .iter()
        .map(|tenant: &DevTenant| -> usize { tenant.accounts.len() })
        .sum();
    if auth_identities.len() != expected_account_count {
        return Err(io::Error::other(format!(
            "expected {expected_account_count} Auth identity mappings, received {}",
            auth_identities.len()
        ))
        .into());
    }
    debug!("Development environment guard accepted APP_ENV=development");

    info!("Connecting to PostgreSQL; migrations must already be applied with the SQLx CLI");
    let db: Arc<DatabaseAdapter> = DatabaseAdapter::new_arc().await;
    for tenant in DEV_TENANTS {
        if let Err(error) = seed_tenant(&db, tenant, &auth_issuer, &auth_identities).await {
            error!(
                "Development tenant seed failed: tenant_slug={} tenant_id={} error={}",
                tenant.slug, tenant.id, error
            );
            return Err(error.into());
        }
    }

    let account_count: usize = expected_account_count;
    info!(
        "Development seed completed successfully: tenants={} accounts={}",
        DEV_TENANTS.len(),
        account_count
    );
    println!("Development data seeded successfully");
    println!("tenant_slugs=acme,acme1,acme2");
    println!("accounts_created_or_verified={account_count}");
    println!("auth_identity_issuer={auth_issuer}");
    println!("auth_identity_linked=true");
    Ok(())
}

async fn seed_tenant(
    db: &Arc<DatabaseAdapter>,
    tenant: &DevTenant,
    auth_issuer: &str,
    auth_identities: &HashMap<String, Uuid>,
) -> Result<(), io::Error> {
    let tenant_id: Uuid = Uuid::parse_str(tenant.id).map_err(io::Error::other)?;
    info!(
        "Provisioning development tenant: tenant_slug={} tenant_id={} display_name={} expected_accounts={}",
        tenant.slug,
        tenant_id,
        tenant.display_name,
        tenant.accounts.len()
    );
    db.provision_tenant(tenant_id, tenant.slug, tenant.display_name)
        .await
        .map_err(io::Error::other)?;
    info!(
        "Tenant provisioned in shared tenant registry: tenant_slug={} tenant_id={}",
        tenant.slug, tenant_id
    );

    let owner_definition: &DevAccount = tenant
        .accounts
        .iter()
        .find(|account: &&DevAccount| account.role == DevRole::Owner)
        .ok_or_else(|| io::Error::other(format!("tenant '{}' has no owner seed definition", tenant.slug)))?;
    let owner_email: String = dev_auth_email(tenant.slug, owner_definition.username);
    let owner: SeedAccount = ensure_account(
        db,
        tenant_id,
        owner_definition.username,
        &owner_email,
        owner_definition.role,
        None,
    )
    .await?;
    ensure_seed_identity(
        db,
        tenant,
        owner_definition,
        &owner,
        auth_issuer,
        auth_identities,
        tenant_id,
    )
    .await?;

    let mut seeded_accounts: Vec<SeedAccount> = vec![owner.clone()];
    for account_definition in tenant
        .accounts
        .iter()
        .filter(|account: &&DevAccount| account.role != DevRole::Owner)
    {
        let account_email: String = dev_auth_email(tenant.slug, account_definition.username);
        let account: SeedAccount = ensure_account(
            db,
            tenant_id,
            account_definition.username,
            &account_email,
            account_definition.role,
            Some(owner.id),
        )
        .await?;
        ensure_seed_identity(
            db,
            tenant,
            account_definition,
            &account,
            auth_issuer,
            auth_identities,
            tenant_id,
        )
        .await?;
        seeded_accounts.push(account);
    }

    seed_branches_and_facilities(db, tenant_id, tenant, owner.id).await?;
    seed_hr_infra(db, tenant_id, tenant, &seeded_accounts, owner.id).await?;
    seed_staffing_business(db, tenant_id, tenant, owner.id).await?;

    info!(
        "Development tenant seed completed: tenant_slug={} tenant_id={} accounts={} branches={} facilities={} employees={}",
        tenant.slug,
        tenant_id,
        tenant.accounts.len(),
        DEV_BRANCHES.len(),
        DEV_BRANCHES
            .iter()
            .map(|branch: &DevBranch| branch.facilities.len())
            .sum::<usize>(),
        seeded_accounts.len()
    );
    Ok(())
}

async fn ensure_seed_identity(
    db: &DatabaseAdapter,
    tenant: &DevTenant,
    account_definition: &DevAccount,
    account: &SeedAccount,
    auth_issuer: &str,
    auth_identities: &HashMap<String, Uuid>,
    tenant_id: Uuid,
) -> Result<(), io::Error> {
    let identity_key: String = format!("{}:{}", tenant.slug, account_definition.username);
    let auth_subject: Uuid = auth_identities.get(&identity_key).copied().ok_or_else(|| {
        io::Error::other(format!(
            "missing Auth identity mapping for development account '{identity_key}'"
        ))
    })?;
    let auth_subject_string: String = auth_subject.to_string();
    ensure_identity(db, auth_issuer, &auth_subject_string, tenant_id, account.id).await?;
    debug!(
        tenant_slug = tenant.slug,
        tenant_id = %tenant_id,
        account_id = %account.id,
        username = account.username,
        role = account.role.as_code(),
        "Development Auth identity linked to application account"
    );
    Ok(())
}

async fn seed_staffing_business(
    db: &DatabaseAdapter,
    tenant_id: Uuid,
    tenant: &DevTenant,
    owner_account_id: Uuid,
) -> Result<(), io::Error> {
    let mut transaction = db.begin_tenant(tenant_id).await.map_err(io::Error::other)?;
    let effective_date =
        NaiveDate::from_ymd_opt(2026, 1, 1).ok_or_else(|| io::Error::other("invalid staffing seed date"))?;
    let job: SeedIdRow = sqlx::query_as!(
        SeedIdRow,
        "SELECT id FROM hr_jobs WHERE tenant_id = $1 AND code = 'employee'",
        tenant_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let job_id: Uuid = job.id;
    let employee: SeedIdRow = sqlx::query_as!(
        SeedIdRow,
        r#"
        SELECT employee.id
        FROM hr_employees AS employee
        INNER JOIN hr_employee_assignments AS assignment
            ON assignment.tenant_id = employee.tenant_id
           AND assignment.employee_id = employee.id
        INNER JOIN accounts AS account
            ON account.tenant_id = employee.tenant_id
           AND account.id = employee.account_id
           AND account.primary_role_code = 'staff'
           AND assignment.job_id = $2
           AND assignment.date_end IS NULL
        WHERE employee.tenant_id = $1 AND employee.status = 'active'
        ORDER BY employee.employee_code
        LIMIT 1
        "#,
        tenant_id,
        job_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let employee_id: Uuid = employee.id;
    let staff_account: SeedIdRow = sqlx::query_as!(
        SeedIdRow,
        r#"SELECT account_id AS "id!" FROM hr_employees WHERE tenant_id = $1 AND id = $2 AND status = 'active'"#,
        tenant_id,
        employee_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let staff_account_id: Uuid = staff_account.id;

    let karaoke_a_id =
        ensure_staffing_customer(&mut transaction, tenant_id, "karaoke-a", "Karaoke A", owner_account_id).await?;
    let karaoke_a_facility_id = ensure_customer_facility(
        &mut transaction,
        tenant_id,
        karaoke_a_id,
        "main",
        "Karaoke A Main",
        owner_account_id,
    )
    .await?;
    let karaoke_b_id =
        ensure_staffing_customer(&mut transaction, tenant_id, "karaoke-b", "Karaoke B", owner_account_id).await?;
    let karaoke_b_facility_id = ensure_customer_facility(
        &mut transaction,
        tenant_id,
        karaoke_b_id,
        "main",
        "Karaoke B Main",
        owner_account_id,
    )
    .await?;

    ensure_staffing_rate(
        &mut transaction,
        tenant_id,
        "karaoke-a-default",
        "Karaoke A default staff rate",
        karaoke_a_id,
        Some(karaoke_a_facility_id),
        None,
        job_id,
        "150000.0000",
        "120000.0000",
        0,
        effective_date,
        owner_account_id,
    )
    .await?;
    ensure_staffing_rate(
        &mut transaction,
        tenant_id,
        "karaoke-b-default",
        "Karaoke B default staff rate",
        karaoke_b_id,
        Some(karaoke_b_facility_id),
        None,
        job_id,
        "180000.0000",
        "135000.0000",
        0,
        effective_date,
        owner_account_id,
    )
    .await?;
    let (customer_bill_rate_id, worker_pay_rate_id) = ensure_staffing_rate(
        &mut transaction,
        tenant_id,
        "karaoke-b-worker-special",
        "Karaoke B worker-specific rate",
        karaoke_b_id,
        Some(karaoke_b_facility_id),
        Some(employee_id),
        job_id,
        "180000.0000",
        "145000.0000",
        100,
        effective_date,
        owner_account_id,
    )
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO business_staffing_employee_eligibilities (
            id, tenant_id, employee_id, job_id, effective_from, notes, created_by_account_id
        )
        SELECT
            MD5($1::UUID::TEXT || ':' || employee.id::TEXT || ':' || $2::UUID::TEXT)::UUID,
            $1,
            employee.id,
            $2,
            $3,
            'Development staffing eligibility',
            $4
        FROM hr_employees AS employee
        INNER JOIN account_roles AS account_role
            ON account_role.tenant_id = employee.tenant_id
           AND account_role.account_id = employee.account_id
           AND account_role.role_code = 'staff'
        WHERE employee.tenant_id = $1
          AND employee.status = 'active'
        ON CONFLICT (tenant_id, employee_id, job_id, effective_from) DO NOTHING
        "#,
        tenant_id,
        job_id,
        effective_date,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    let shift_id = Uuid::from_u128(tenant_id.as_u128() ^ DEV_STAFFING_SHIFT_ID_NAMESPACE);
    let assignment_id = Uuid::from_u128(tenant_id.as_u128() ^ DEV_STAFFING_ASSIGNMENT_ID_NAMESPACE);
    sqlx::query!(
        r#"
        INSERT INTO business_staffing_shifts (
            id, tenant_id, customer_id, customer_facility_id, job_id, starts_at, ends_at,
            required_workers, status, notes, created_by_account_id, updated_by_account_id
        )
        VALUES (
            $1, $2, $3, $4, $5, CURRENT_TIMESTAMP - INTERVAL '15 minutes',
            CURRENT_TIMESTAMP + INTERVAL '6 hours', 1, 'filled',
            'Development shift for testing employee start and end actions', $6, $6
        )
        ON CONFLICT (id) DO NOTHING
        "#,
        shift_id,
        tenant_id,
        karaoke_b_id,
        karaoke_b_facility_id,
        job_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO business_shift_assignments (
            id, tenant_id, shift_id, employee_id, customer_bill_rate_id, worker_pay_rate_id,
            rate_source, currency, bill_hourly_rate_snapshot, worker_hourly_rate_snapshot,
            created_by_account_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'configured', 'VND', 180000, 145000, $7)
        ON CONFLICT (id) DO NOTHING
        "#,
        assignment_id,
        tenant_id,
        shift_id,
        employee_id,
        customer_bill_rate_id,
        worker_pay_rate_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    let urgent_batch_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_URGENT_BATCH_ID_NAMESPACE);
    let urgent_report_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_URGENT_REPORT_ID_NAMESPACE);
    let urgent_session_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_URGENT_SESSION_ID_NAMESPACE);
    let urgent_customer_record_id: Uuid =
        Uuid::from_u128(tenant_id.as_u128() ^ DEV_URGENT_CUSTOMER_RECORD_ID_NAMESPACE);
    let urgent_batch_insert: sqlx::postgres::PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_urgent_work_batches (
            id, tenant_id, actor_account_id, claimed_customer_facility_id, idempotency_key
        ) VALUES ($1, $2, $3, $4, $1)
        ON CONFLICT (id) DO NOTHING
        "#,
        urgent_batch_id,
        tenant_id,
        staff_account_id,
        karaoke_a_facility_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let urgent_report_insert: sqlx::postgres::PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_urgent_work_reports (
            id, tenant_id, start_batch_id, employee_id, claimed_customer_facility_id,
            status, created_by_account_id
        ) VALUES ($1, $2, $3, $4, $5, 'completed', $6)
        ON CONFLICT (id) DO NOTHING
        "#,
        urgent_report_id,
        tenant_id,
        urgent_batch_id,
        employee_id,
        karaoke_a_facility_id,
        staff_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let urgent_session_insert: sqlx::postgres::PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_urgent_work_sessions (
            id, tenant_id, report_id, employee_id, started_at, ended_at,
            end_idempotency_key, started_by_account_id, start_source,
            ended_by_account_id, end_source
        ) VALUES (
            $1, $2, $3, $4, CURRENT_TIMESTAMP - INTERVAL '5 hours',
            CURRENT_TIMESTAMP - INTERVAL '1 hour', $1, $5, 'self', $5, 'self'
        )
        ON CONFLICT (id) DO NOTHING
        "#,
        urgent_session_id,
        tenant_id,
        urgent_report_id,
        employee_id,
        staff_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let urgent_customer_record_insert: sqlx::postgres::PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_urgent_customer_work_records (
            id, tenant_id, report_id, confirmed_customer_facility_id,
            confirmed_started_at, confirmed_ended_at, customer_reference,
            notes, recorded_by_account_id
        )
        SELECT $1, $2, $3, $4, session.started_at, session.ended_at,
               'DEV-MATCHED-001', 'Development matched urgent evidence', $5
        FROM business_urgent_work_sessions AS session
        WHERE session.tenant_id = $2 AND session.report_id = $3 AND session.ended_at IS NOT NULL
        ON CONFLICT (id) DO NOTHING
        "#,
        urgent_customer_record_id,
        tenant_id,
        urgent_report_id,
        karaoke_a_facility_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    trace!(
        tenant_slug = tenant.slug,
        tenant_id = %tenant_id,
        urgent_report_id = %urgent_report_id,
        batch_rows = urgent_batch_insert.rows_affected(),
        report_rows = urgent_report_insert.rows_affected(),
        session_rows = urgent_session_insert.rows_affected(),
        customer_rows = urgent_customer_record_insert.rows_affected(),
        "Development matched urgent-work evidence ensured"
    );

    transaction.commit().await.map_err(io::Error::other)?;
    info!(
        "Development staffing business committed: tenant_slug={} tenant_id={} sample_employee_id={} sample_shift_id={} sample_assignment_id={} urgent_report_id={}",
        tenant.slug, tenant_id, employee_id, shift_id, assignment_id, urgent_report_id
    );
    Ok(())
}

async fn ensure_staffing_customer(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    code: &str,
    name: &str,
    owner_account_id: Uuid,
) -> Result<Uuid, io::Error> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO business_customers (
            id, tenant_id, code, name, status, created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, $3, $4, 'active', $5, $5)
        ON CONFLICT (tenant_id, lower(code)) DO UPDATE
        SET name = EXCLUDED.name,
            status = 'active',
            updated_at = CURRENT_TIMESTAMP,
            updated_by_account_id = EXCLUDED.updated_by_account_id
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        code,
        name,
        owner_account_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)
}

async fn ensure_customer_facility(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    customer_id: Uuid,
    code: &str,
    name: &str,
    owner_account_id: Uuid,
) -> Result<Uuid, io::Error> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO business_customer_facilities (
            id, tenant_id, customer_id, code, name, time_zone, status,
            created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, $3, $4, $5, 'Asia/Bangkok', 'active', $6, $6)
        ON CONFLICT (tenant_id, customer_id, lower(code)) DO UPDATE
        SET name = EXCLUDED.name,
            status = 'active',
            updated_at = CURRENT_TIMESTAMP,
            updated_by_account_id = EXCLUDED.updated_by_account_id
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        customer_id,
        code,
        name,
        owner_account_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)
}

#[allow(clippy::too_many_arguments)]
async fn ensure_staffing_rate(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    code: &str,
    name: &str,
    customer_id: Uuid,
    customer_facility_id: Option<Uuid>,
    employee_id: Option<Uuid>,
    job_id: Uuid,
    bill_hourly_rate: &str,
    worker_hourly_rate: &str,
    priority: i16,
    effective_from: NaiveDate,
    owner_account_id: Uuid,
) -> Result<(Uuid, Uuid), io::Error> {
    let customer_bill_rate_id: Uuid = ensure_staffing_hourly_rate(
        transaction,
        tenant_id,
        "customer_bill",
        &format!("{code}-bill"),
        &format!("{name} - customer bill"),
        Some(customer_id),
        customer_facility_id,
        employee_id,
        job_id,
        bill_hourly_rate,
        priority,
        effective_from,
        owner_account_id,
    )
    .await?;
    let worker_pay_rate_id: Uuid = ensure_staffing_hourly_rate(
        transaction,
        tenant_id,
        "worker_pay",
        &format!("{code}-pay"),
        &format!("{name} - worker pay"),
        Some(customer_id),
        customer_facility_id,
        employee_id,
        job_id,
        worker_hourly_rate,
        priority,
        effective_from,
        owner_account_id,
    )
    .await?;
    Ok((customer_bill_rate_id, worker_pay_rate_id))
}

#[allow(clippy::too_many_arguments)]
async fn ensure_staffing_hourly_rate(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    rate_kind: &str,
    code: &str,
    name: &str,
    customer_id: Option<Uuid>,
    customer_facility_id: Option<Uuid>,
    employee_id: Option<Uuid>,
    job_id: Uuid,
    hourly_rate: &str,
    priority: i16,
    effective_from: NaiveDate,
    owner_account_id: Uuid,
) -> Result<Uuid, io::Error> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO business_staffing_rates (
            id, tenant_id, rate_kind, code, name, customer_id, customer_facility_id,
            employee_id, job_id, currency, hourly_rate, priority, effective_from,
            is_active, created_by_account_id
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, 'VND',
            $10::TEXT::NUMERIC, $11, $12, TRUE, $13
        )
        ON CONFLICT (tenant_id, rate_kind, code, effective_from) DO UPDATE
        SET name = EXCLUDED.name,
            customer_id = EXCLUDED.customer_id,
            customer_facility_id = EXCLUDED.customer_facility_id,
            employee_id = EXCLUDED.employee_id,
            job_id = EXCLUDED.job_id,
            currency = EXCLUDED.currency,
            hourly_rate = EXCLUDED.hourly_rate,
            priority = EXCLUDED.priority,
            effective_to = NULL,
            is_active = TRUE
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        rate_kind,
        code,
        name,
        customer_id,
        customer_facility_id,
        employee_id,
        job_id,
        hourly_rate,
        priority,
        effective_from,
        owner_account_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)
}

async fn seed_hr_infra(
    db: &DatabaseAdapter,
    tenant_id: Uuid,
    tenant: &DevTenant,
    accounts: &[SeedAccount],
    owner_account_id: Uuid,
) -> Result<(), io::Error> {
    let mut transaction = db.begin_tenant(tenant_id).await.map_err(io::Error::other)?;
    let effective_date: NaiveDate =
        NaiveDate::from_ymd_opt(2026, 1, 1).ok_or_else(|| io::Error::other("invalid HR seed effective date"))?;
    info!(
        "Seeding Odoo-inspired HR infra: tenant_slug={} tenant_id={} employees={} effective_date={}",
        tenant.slug,
        tenant_id,
        accounts.len(),
        effective_date
    );

    let administration_department_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO hr_departments (
            id, tenant_id, code, name, status, created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, 'administration', 'Administration', 'active', $3, $3)
        ON CONFLICT (tenant_id, lower(code)) DO UPDATE
        SET name = EXCLUDED.name,
            status = 'active',
            updated_at = CURRENT_TIMESTAMP,
            updated_by_account_id = EXCLUDED.updated_by_account_id
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        owner_account_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let operations_department_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO hr_departments (
            id, tenant_id, code, name, status, created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, 'operations', 'Operations', 'active', $3, $3)
        ON CONFLICT (tenant_id, lower(code)) DO UPDATE
        SET name = EXCLUDED.name,
            status = 'active',
            updated_at = CURRENT_TIMESTAMP,
            updated_by_account_id = EXCLUDED.updated_by_account_id
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        owner_account_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    let owner_job_id: Uuid = ensure_dev_job(
        &mut transaction,
        tenant_id,
        "owner",
        "Owner",
        administration_department_id,
        owner_account_id,
    )
    .await?;
    let supervisor_job_id: Uuid = ensure_dev_job(
        &mut transaction,
        tenant_id,
        "supervisor",
        "Supervisor",
        operations_department_id,
        owner_account_id,
    )
    .await?;
    let employee_job_id: Uuid = ensure_dev_job(
        &mut transaction,
        tenant_id,
        "employee",
        "Employee",
        operations_department_id,
        owner_account_id,
    )
    .await?;

    let mut employee_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for account in accounts {
        let employee_code: String = account.username.to_ascii_lowercase();
        let work_email: String = format!("{}@{}.dev", employee_code, tenant.slug);
        let employee_id: Uuid = sqlx::query_scalar!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, account_id, employee_code, display_name, work_email, status, hire_date,
                created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $8, $8)
            ON CONFLICT (tenant_id, lower(employee_code)) DO UPDATE
            SET account_id = EXCLUDED.account_id,
                display_name = EXCLUDED.display_name,
                work_email = EXCLUDED.work_email,
                status = 'active',
                termination_date = NULL,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = EXCLUDED.updated_by_account_id
            RETURNING id
            "#,
            Uuid::new_v4(),
            tenant_id,
            account.id,
            employee_code,
            account.username,
            work_email,
            effective_date,
            owner_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(io::Error::other)?;
        employee_ids.insert(account.id, employee_id);
        debug!(
            "Development employee ensured: tenant_slug={} employee_id={} employee_code={} account_id={} role={}",
            tenant.slug,
            employee_id,
            employee_code,
            account.id,
            account.role.as_code()
        );
    }

    let owner_employee_id: Uuid = *employee_ids
        .get(&owner_account_id)
        .ok_or_else(|| io::Error::other("seeded owner employee was not found"))?;
    let supervisor_employee_ids: Vec<Uuid> = accounts
        .iter()
        .filter(|account: &&SeedAccount| account.role == DevRole::Manager)
        .filter_map(|account: &SeedAccount| employee_ids.get(&account.id).copied())
        .collect();
    let operations_manager_id: Uuid = supervisor_employee_ids.first().copied().unwrap_or(owner_employee_id);

    sqlx::query!(
        r#"
        UPDATE hr_departments
        SET manager_employee_id = CASE code
                WHEN 'administration' THEN $2
                WHEN 'operations' THEN $3
                ELSE manager_employee_id
            END,
            updated_at = CURRENT_TIMESTAMP,
            updated_by_account_id = $4
        WHERE tenant_id = $1
          AND code IN ('administration', 'operations')
        "#,
        tenant_id,
        owner_employee_id,
        operations_manager_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    let head_office_branch_id: Uuid = sqlx::query_scalar!(
        r#"SELECT id FROM branches WHERE tenant_id = $1 AND code = 'head-office'"#,
        tenant_id
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let north_branch_id: Uuid = sqlx::query_scalar!(
        r#"SELECT id FROM branches WHERE tenant_id = $1 AND code = 'north-branch'"#,
        tenant_id
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let head_office_facility_id: Uuid = sqlx::query_scalar!(
        r#"SELECT id FROM facilities WHERE tenant_id = $1 AND branch_id = $2 AND code = 'main-office'"#,
        tenant_id,
        head_office_branch_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let north_facility_id: Uuid = sqlx::query_scalar!(
        r#"SELECT id FROM facilities WHERE tenant_id = $1 AND branch_id = $2 AND code = 'office'"#,
        tenant_id,
        north_branch_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let head_office_warehouse_id: Uuid = sqlx::query_scalar!(
        r#"SELECT id FROM facilities WHERE tenant_id = $1 AND branch_id = $2 AND code = 'warehouse'"#,
        tenant_id,
        head_office_branch_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let north_warehouse_id: Uuid = sqlx::query_scalar!(
        r#"SELECT id FROM facilities WHERE tenant_id = $1 AND branch_id = $2 AND code = 'warehouse'"#,
        tenant_id,
        north_branch_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    let mut supervisor_index: usize = 0;
    let mut employee_index: usize = 0;
    let mut employee_facility_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for account in accounts {
        let employee_id: Uuid = *employee_ids
            .get(&account.id)
            .ok_or_else(|| io::Error::other("seeded employee account mapping was not found"))?;
        let (branch_id, facility_id, department_id, job_id, manager_employee_id) = match account.role {
            DevRole::Owner => (
                head_office_branch_id,
                head_office_facility_id,
                administration_department_id,
                owner_job_id,
                None,
            ),
            DevRole::Manager => {
                let use_north_branch: bool = supervisor_index % 2 == 1;
                supervisor_index += 1;
                (
                    if use_north_branch {
                        north_branch_id
                    } else {
                        head_office_branch_id
                    },
                    if use_north_branch {
                        north_facility_id
                    } else {
                        head_office_facility_id
                    },
                    operations_department_id,
                    supervisor_job_id,
                    Some(owner_employee_id),
                )
            }
            DevRole::Staff => {
                let use_north_branch: bool = employee_index % 2 == 1;
                let manager_employee_id: Uuid = supervisor_employee_ids
                    .get(employee_index % supervisor_employee_ids.len().max(1))
                    .copied()
                    .unwrap_or(owner_employee_id);
                employee_index += 1;
                (
                    if use_north_branch {
                        north_branch_id
                    } else {
                        head_office_branch_id
                    },
                    if use_north_branch {
                        north_warehouse_id
                    } else {
                        head_office_warehouse_id
                    },
                    operations_department_id,
                    employee_job_id,
                    Some(manager_employee_id),
                )
            }
        };
        let assignment_id: Uuid = sqlx::query_scalar!(
            r#"
            INSERT INTO hr_employee_assignments (
                id, tenant_id, employee_id, branch_id, facility_id, department_id, job_id, manager_employee_id,
                date_start, is_primary, created_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, TRUE, $10)
            ON CONFLICT (tenant_id, employee_id) WHERE is_primary AND date_end IS NULL DO UPDATE
            SET branch_id = EXCLUDED.branch_id,
                facility_id = EXCLUDED.facility_id,
                department_id = EXCLUDED.department_id,
                job_id = EXCLUDED.job_id,
                manager_employee_id = EXCLUDED.manager_employee_id,
                date_start = EXCLUDED.date_start
            RETURNING id
            "#,
            Uuid::new_v4(),
            tenant_id,
            employee_id,
            branch_id,
            facility_id,
            department_id,
            job_id,
            manager_employee_id,
            effective_date,
            owner_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(io::Error::other)?;
        employee_facility_ids.insert(employee_id, facility_id);
        debug!(
            "Development HR assignment ensured: tenant_slug={} employee_id={} assignment_id={} branch_id={} facility_id={} manager_employee_id={:?}",
            tenant.slug, employee_id, assignment_id, branch_id, facility_id, manager_employee_id
        );
    }

    let standard_schedule_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO hr_working_schedules (
            id, tenant_id, code, name, time_zone, status, created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, 'standard-40', 'Standard 40 Hours', 'Asia/Bangkok', 'active', $3, $3)
        ON CONFLICT (tenant_id, lower(code)) DO UPDATE
        SET name = EXCLUDED.name,
            time_zone = EXCLUDED.time_zone,
            status = 'active',
            updated_at = CURRENT_TIMESTAMP,
            updated_by_account_id = EXCLUDED.updated_by_account_id
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        owner_account_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        "DELETE FROM hr_working_schedule_periods WHERE tenant_id = $1 AND schedule_id = $2",
        tenant_id,
        standard_schedule_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let work_start: NaiveTime =
        NaiveTime::from_hms_opt(8, 0, 0).ok_or_else(|| io::Error::other("invalid seeded work start time"))?;
    let work_end: NaiveTime =
        NaiveTime::from_hms_opt(17, 0, 0).ok_or_else(|| io::Error::other("invalid seeded work end time"))?;
    for weekday in 1_i16..=5_i16 {
        sqlx::query!(
            r#"
            INSERT INTO hr_working_schedule_periods (
                id, tenant_id, schedule_id, weekday, start_time, end_time, spans_next_day, unpaid_break_minutes
            )
            VALUES ($1, $2, $3, $4, $5, $6, FALSE, 60)
            "#,
            Uuid::new_v4(),
            tenant_id,
            standard_schedule_id,
            weekday,
            work_start,
            work_end,
        )
        .execute(transaction.connection())
        .await
        .map_err(io::Error::other)?;
    }
    for employee_id in employee_ids.values() {
        let schedule_assignment_id: Uuid = sqlx::query_scalar!(
            r#"
            INSERT INTO hr_employee_schedule_assignments (
                id, tenant_id, employee_id, schedule_id, date_start, created_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, employee_id) WHERE date_end IS NULL DO UPDATE
            SET schedule_id = EXCLUDED.schedule_id,
                date_start = EXCLUDED.date_start
            RETURNING id
            "#,
            Uuid::new_v4(),
            tenant_id,
            employee_id,
            standard_schedule_id,
            effective_date,
            owner_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(io::Error::other)?;
        debug!(
            "Development working schedule assignment ensured: tenant_slug={} employee_id={} schedule_id={} assignment_id={} effective_date={}",
            tenant.slug, employee_id, standard_schedule_id, schedule_assignment_id, effective_date
        );
    }

    seed_payroll_configuration(
        &mut transaction,
        tenant_id,
        tenant,
        accounts,
        &employee_ids,
        owner_account_id,
        effective_date,
    )
    .await?;

    let (completed_attendance_sessions, open_attendance_sessions): (usize, usize) = seed_attendance_sessions(
        &mut transaction,
        tenant_id,
        tenant,
        accounts,
        &employee_ids,
        &employee_facility_ids,
    )
    .await?;

    transaction.commit().await.map_err(io::Error::other)?;
    info!(
        "Development HR infra committed: tenant_slug={} tenant_id={} departments=2 jobs=3 employees={} assignments={} working_schedules=1 schedule_assignments={} completed_attendance_sessions={} open_attendance_sessions={}",
        tenant.slug,
        tenant_id,
        accounts.len(),
        accounts.len(),
        accounts.len(),
        completed_attendance_sessions,
        open_attendance_sessions
    );
    Ok(())
}

async fn seed_payroll_configuration(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    tenant: &DevTenant,
    accounts: &[SeedAccount],
    employee_ids: &HashMap<Uuid, Uuid>,
    owner_account_id: Uuid,
    effective_date: NaiveDate,
) -> Result<(), io::Error> {
    for account in accounts {
        let employee_id: Uuid = employee_ids
            .get(&account.id)
            .copied()
            .ok_or_else(|| io::Error::other("seeded payroll employee mapping was not found"))?;
        match account.role {
            DevRole::Staff => {
                sqlx::query!(
                    r#"
                    INSERT INTO hr_employee_compensations (
                        id, tenant_id, employee_id, currency, pay_basis, hourly_rate,
                        effective_from, created_by_account_id
                    )
                    VALUES ($1, $2, $3, 'VND', 'hourly', 120000, $4, $5)
                    ON CONFLICT (tenant_id, employee_id, effective_from) DO UPDATE
                    SET currency = 'VND',
                        pay_basis = 'hourly',
                        hourly_rate = 120000,
                        monthly_rate = NULL,
                        standard_monthly_hours = NULL,
                        effective_to = NULL,
                        created_by_account_id = EXCLUDED.created_by_account_id
                    "#,
                    Uuid::new_v4(),
                    tenant_id,
                    employee_id,
                    effective_date,
                    owner_account_id,
                )
                .execute(transaction.connection())
                .await
                .map_err(io::Error::other)?;
            }
            DevRole::Manager | DevRole::Owner => {
                let monthly_rate: &str = if account.role == DevRole::Owner {
                    "50000000"
                } else {
                    "30000000"
                };
                sqlx::query!(
                    r#"
                    INSERT INTO hr_employee_compensations (
                        id, tenant_id, employee_id, currency, pay_basis, monthly_rate,
                        standard_monthly_hours, effective_from, created_by_account_id
                    )
                    VALUES ($1, $2, $3, 'VND', 'monthly', $4::TEXT::NUMERIC, 160, $5, $6)
                    ON CONFLICT (tenant_id, employee_id, effective_from) DO UPDATE
                    SET currency = 'VND',
                        pay_basis = 'monthly',
                        hourly_rate = NULL,
                        monthly_rate = EXCLUDED.monthly_rate,
                        standard_monthly_hours = 160,
                        effective_to = NULL,
                        created_by_account_id = EXCLUDED.created_by_account_id
                    "#,
                    Uuid::new_v4(),
                    tenant_id,
                    employee_id,
                    monthly_rate,
                    effective_date,
                    owner_account_id,
                )
                .execute(transaction.connection())
                .await
                .map_err(io::Error::other)?;
            }
        }
    }

    let warehouse_facilities = sqlx::query!(
        r#"
        SELECT facility.id, branch.code AS branch_code
        FROM facilities AS facility
        INNER JOIN branches AS branch
            ON branch.tenant_id = facility.tenant_id AND branch.id = facility.branch_id
        WHERE facility.tenant_id = $1 AND facility.code = 'warehouse'
        ORDER BY branch.code
        "#,
        tenant_id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let warehouse_rule_count: usize = warehouse_facilities.len();
    for facility in warehouse_facilities {
        let rule_code: String = format!("warehouse-{}", facility.branch_code);
        sqlx::query!(
            r#"
            INSERT INTO payroll_facility_rate_rules (
                id, tenant_id, code, name, facility_id, base_multiplier, hourly_adjustment,
                priority, effective_from, is_active, created_by_account_id
            )
            VALUES ($1, $2, $3, 'Warehouse premium', $4, 1.15, 0, 10, $5, TRUE, $6)
            ON CONFLICT (tenant_id, code, effective_from) DO UPDATE
            SET name = EXCLUDED.name,
                facility_id = EXCLUDED.facility_id,
                employee_id = NULL,
                base_multiplier = EXCLUDED.base_multiplier,
                hourly_adjustment = EXCLUDED.hourly_adjustment,
                priority = EXCLUDED.priority,
                effective_to = NULL,
                is_active = TRUE,
                created_by_account_id = EXCLUDED.created_by_account_id
            "#,
            Uuid::new_v4(),
            tenant_id,
            rule_code,
            facility.id,
            effective_date,
            owner_account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(io::Error::other)?;
    }

    sqlx::query!(
        r#"
        INSERT INTO payroll_time_band_rules (
            id, tenant_id, code, name, weekdays, start_time, end_time, spans_next_day,
            premium_multiplier, hourly_adjustment, priority, effective_from, is_active,
            created_by_account_id
        )
        VALUES (
            $1, $2, 'night-shift', 'Night shift premium', ARRAY[1, 2, 3, 4, 5, 6, 7]::SMALLINT[],
            TIME '22:00', TIME '06:00', TRUE, 0.25, 0, 10, $3, TRUE, $4
        )
        ON CONFLICT (tenant_id, code, effective_from) DO UPDATE
        SET name = EXCLUDED.name,
            weekdays = EXCLUDED.weekdays,
            start_time = EXCLUDED.start_time,
            end_time = EXCLUDED.end_time,
            spans_next_day = EXCLUDED.spans_next_day,
            premium_multiplier = EXCLUDED.premium_multiplier,
            hourly_adjustment = EXCLUDED.hourly_adjustment,
            priority = EXCLUDED.priority,
            effective_to = NULL,
            is_active = TRUE,
            created_by_account_id = EXCLUDED.created_by_account_id
        "#,
        Uuid::new_v4(),
        tenant_id,
        effective_date,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    for (code, name, threshold_minutes, premium_multiplier) in [
        ("daily-ot-after-8h", "Daily overtime after 8 hours", 480_i32, "0.50"),
        (
            "daily-ot-after-12h",
            "Additional overtime after 12 hours",
            720_i32,
            "0.50",
        ),
    ] {
        sqlx::query!(
            r#"
            INSERT INTO payroll_overtime_rules (
                id, tenant_id, code, name, threshold_minutes, premium_multiplier,
                hourly_adjustment, priority, effective_from, is_active, created_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6::TEXT::NUMERIC, 0, 10, $7, TRUE, $8)
            ON CONFLICT (tenant_id, code, effective_from) DO UPDATE
            SET name = EXCLUDED.name,
                threshold_minutes = EXCLUDED.threshold_minutes,
                premium_multiplier = EXCLUDED.premium_multiplier,
                hourly_adjustment = EXCLUDED.hourly_adjustment,
                priority = EXCLUDED.priority,
                effective_to = NULL,
                is_active = TRUE,
                created_by_account_id = EXCLUDED.created_by_account_id
            "#,
            Uuid::new_v4(),
            tenant_id,
            code,
            name,
            threshold_minutes,
            premium_multiplier,
            effective_date,
            owner_account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(io::Error::other)?;
    }

    info!(
        "Development payroll configuration ensured: tenant_slug={} tenant_id={} compensations={} warehouse_rules={} time_rules=1 overtime_rules=2 currency=VND",
        tenant.slug,
        tenant_id,
        accounts.len(),
        warehouse_rule_count
    );
    Ok(())
}

async fn seed_attendance_sessions(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    tenant: &DevTenant,
    accounts: &[SeedAccount],
    employee_ids: &HashMap<Uuid, Uuid>,
    employee_facility_ids: &HashMap<Uuid, Uuid>,
) -> Result<(usize, usize), io::Error> {
    let morning_start: NaiveTime =
        NaiveTime::from_hms_opt(8, 5, 0).ok_or_else(|| io::Error::other("invalid attendance seed time"))?;
    let morning_end: NaiveTime =
        NaiveTime::from_hms_opt(12, 0, 0).ok_or_else(|| io::Error::other("invalid attendance seed time"))?;
    let afternoon_start: NaiveTime =
        NaiveTime::from_hms_opt(13, 0, 0).ok_or_else(|| io::Error::other("invalid attendance seed time"))?;
    let afternoon_end: NaiveTime =
        NaiveTime::from_hms_opt(17, 15, 0).ok_or_else(|| io::Error::other("invalid attendance seed time"))?;
    let full_day_start: NaiveTime =
        NaiveTime::from_hms_opt(8, 12, 0).ok_or_else(|| io::Error::other("invalid attendance seed time"))?;
    let full_day_end: NaiveTime =
        NaiveTime::from_hms_opt(17, 3, 0).ok_or_else(|| io::Error::other("invalid attendance seed time"))?;
    let night_start: NaiveTime =
        NaiveTime::from_hms_opt(23, 0, 0).ok_or_else(|| io::Error::other("invalid attendance seed time"))?;
    let night_end: NaiveTime =
        NaiveTime::from_hms_opt(6, 0, 0).ok_or_else(|| io::Error::other("invalid attendance seed time"))?;
    let mut completed_count: usize = 0;
    let mut open_count: usize = 0;

    for account in accounts
        .iter()
        .filter(|account: &&SeedAccount| account.role == DevRole::Staff)
    {
        let employee_id: Uuid = employee_ids
            .get(&account.id)
            .copied()
            .ok_or_else(|| io::Error::other("seeded attendance employee mapping was not found"))?;
        let facility_id: Uuid = employee_facility_ids
            .get(&employee_id)
            .copied()
            .ok_or_else(|| io::Error::other("seeded attendance facility mapping was not found"))?;

        ensure_completed_attendance_session(
            transaction,
            tenant_id,
            employee_id,
            facility_id,
            account.id,
            dev_attendance_session_id(account.id, 1),
            2,
            full_day_start,
            full_day_end,
        )
        .await?;
        ensure_completed_attendance_session(
            transaction,
            tenant_id,
            employee_id,
            facility_id,
            account.id,
            dev_attendance_session_id(account.id, 2),
            1,
            morning_start,
            morning_end,
        )
        .await?;
        ensure_completed_attendance_session(
            transaction,
            tenant_id,
            employee_id,
            facility_id,
            account.id,
            dev_attendance_session_id(account.id, 3),
            1,
            afternoon_start,
            afternoon_end,
        )
        .await?;
        ensure_completed_overnight_attendance_session(
            transaction,
            tenant_id,
            employee_id,
            facility_id,
            account.id,
            dev_attendance_session_id(account.id, 5),
            3,
            night_start,
            night_end,
        )
        .await?;
        completed_count += 4;

        if account.username.to_ascii_lowercase().ends_with("employee1") {
            let session_id: Uuid = dev_attendance_session_id(account.id, 4);
            let seeded: bool = sqlx::query_scalar!(
                r#"
                INSERT INTO hr_attendance_sessions (
                    id, tenant_id, employee_id, facility_id, check_in_at, check_in_by_account_id
                )
                SELECT
                    $1,
                    $2,
                    $3,
                    $5,
                    LEAST(
                        (
                            (CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Bangkok')::date + TIME '08:00'
                        ) AT TIME ZONE 'Asia/Bangkok',
                        CURRENT_TIMESTAMP - INTERVAL '15 minutes'
                    ),
                    $4
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM hr_attendance_sessions
                    WHERE tenant_id = $2
                      AND employee_id = $3
                      AND check_out_at IS NULL
                      AND id <> $1
                )
                ON CONFLICT (id) DO UPDATE
                SET employee_id = EXCLUDED.employee_id,
                    facility_id = EXCLUDED.facility_id,
                    check_in_at = EXCLUDED.check_in_at,
                    check_out_at = NULL,
                    check_in_by_account_id = EXCLUDED.check_in_by_account_id,
                    check_out_by_account_id = NULL,
                    updated_at = CURRENT_TIMESTAMP
                RETURNING TRUE AS "seeded!"
                "#,
                session_id,
                tenant_id,
                employee_id,
                account.id,
                facility_id,
            )
            .fetch_optional(transaction.connection())
            .await
            .map_err(io::Error::other)?
            .unwrap_or(false);
            if seeded {
                open_count += 1;
            } else {
                info!(
                    "Open attendance fixture skipped because another session is already open: tenant_slug={} employee_id={} username={}",
                    tenant.slug, employee_id, account.username
                );
            }
        }
    }

    info!(
        "Development attendance ensured: tenant_slug={} tenant_id={} completed_sessions={} open_sessions={}",
        tenant.slug, tenant_id, completed_count, open_count
    );
    Ok((completed_count, open_count))
}

#[allow(clippy::too_many_arguments)]
async fn ensure_completed_attendance_session(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    employee_id: Uuid,
    facility_id: Uuid,
    account_id: Uuid,
    session_id: Uuid,
    days_ago: i32,
    check_in_time: NaiveTime,
    check_out_time: NaiveTime,
) -> Result<(), io::Error> {
    sqlx::query!(
        r#"
        INSERT INTO hr_attendance_sessions (
            id, tenant_id, employee_id, facility_id, check_in_at, check_out_at,
            check_in_by_account_id, check_out_by_account_id
        )
        VALUES (
            $1,
            $2,
            $3,
            $8,
            (
                (
                    (CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Bangkok')::date - $5::integer
                ) + $6::time without time zone
            ) AT TIME ZONE 'Asia/Bangkok',
            (
                (
                    (CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Bangkok')::date - $5::integer
                ) + $7::time without time zone
            ) AT TIME ZONE 'Asia/Bangkok',
            $4,
            $4
        )
        ON CONFLICT (id) DO UPDATE
        SET employee_id = EXCLUDED.employee_id,
            facility_id = EXCLUDED.facility_id,
            check_in_at = EXCLUDED.check_in_at,
            check_out_at = EXCLUDED.check_out_at,
            check_in_by_account_id = EXCLUDED.check_in_by_account_id,
            check_out_by_account_id = EXCLUDED.check_out_by_account_id,
            updated_at = CURRENT_TIMESTAMP
        "#,
        session_id,
        tenant_id,
        employee_id,
        account_id,
        days_ago,
        check_in_time,
        check_out_time,
        facility_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    Ok(())
}

fn dev_attendance_session_id(account_id: Uuid, scenario: u8) -> Uuid {
    Uuid::from_u128(account_id.as_u128() ^ DEV_ATTENDANCE_ID_NAMESPACE ^ u128::from(scenario))
}

#[allow(clippy::too_many_arguments)]
async fn ensure_completed_overnight_attendance_session(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    employee_id: Uuid,
    facility_id: Uuid,
    account_id: Uuid,
    session_id: Uuid,
    start_days_ago: i32,
    check_in_time: NaiveTime,
    check_out_time: NaiveTime,
) -> Result<(), io::Error> {
    sqlx::query!(
        r#"
        INSERT INTO hr_attendance_sessions (
            id, tenant_id, employee_id, facility_id, check_in_at, check_out_at,
            check_in_by_account_id, check_out_by_account_id
        )
        VALUES (
            $1,
            $2,
            $3,
            $8,
            (
                ((CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Bangkok')::date - $5::integer) + $6::time
            ) AT TIME ZONE 'Asia/Bangkok',
            (
                ((CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Bangkok')::date - $5::integer + 1) + $7::time
            ) AT TIME ZONE 'Asia/Bangkok',
            $4,
            $4
        )
        ON CONFLICT (id) DO UPDATE
        SET employee_id = EXCLUDED.employee_id,
            facility_id = EXCLUDED.facility_id,
            check_in_at = EXCLUDED.check_in_at,
            check_out_at = EXCLUDED.check_out_at,
            check_in_by_account_id = EXCLUDED.check_in_by_account_id,
            check_out_by_account_id = EXCLUDED.check_out_by_account_id,
            updated_at = CURRENT_TIMESTAMP
        "#,
        session_id,
        tenant_id,
        employee_id,
        account_id,
        start_days_ago,
        check_in_time,
        check_out_time,
        facility_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    Ok(())
}

async fn ensure_dev_job(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    code: &str,
    name: &str,
    department_id: Uuid,
    owner_account_id: Uuid,
) -> Result<Uuid, io::Error> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO hr_jobs (
            id, tenant_id, code, name, department_id, status, created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, $3, $4, $5, 'active', $6, $6)
        ON CONFLICT (tenant_id, lower(code)) DO UPDATE
        SET name = EXCLUDED.name,
            department_id = EXCLUDED.department_id,
            status = 'active',
            updated_at = CURRENT_TIMESTAMP,
            updated_by_account_id = EXCLUDED.updated_by_account_id
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        code,
        name,
        department_id,
        owner_account_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)
}

async fn seed_branches_and_facilities(
    db: &DatabaseAdapter,
    tenant_id: Uuid,
    tenant: &DevTenant,
    owner_account_id: Uuid,
) -> Result<(), io::Error> {
    let mut transaction = db.begin_tenant(tenant_id).await.map_err(io::Error::other)?;
    info!(
        "Seeding development branch hierarchy: tenant_slug={} tenant_id={} branches={}",
        tenant.slug,
        tenant_id,
        DEV_BRANCHES.len()
    );

    for branch in DEV_BRANCHES {
        let branch_id: Uuid = Uuid::new_v4();
        let branch_id: Uuid = sqlx::query_scalar!(
            r#"
            INSERT INTO branches (
                id, tenant_id, code, name, time_zone, created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $6)
            ON CONFLICT (tenant_id, lower(code)) DO UPDATE
            SET name = EXCLUDED.name,
                time_zone = EXCLUDED.time_zone,
                status = 'active',
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = EXCLUDED.updated_by_account_id
            RETURNING id
            "#,
            branch_id,
            tenant_id,
            branch.code,
            branch.name,
            branch.time_zone,
            owner_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(io::Error::other)?;
        debug!(
            "Development branch ensured: tenant_slug={} branch_id={} branch_code={} facilities={}",
            tenant.slug,
            branch_id,
            branch.code,
            branch.facilities.len()
        );

        for facility in branch.facilities {
            let facility_id: Uuid = Uuid::new_v4();
            let facility_id: Uuid = sqlx::query_scalar!(
                r#"
                INSERT INTO facilities (
                    id, tenant_id, branch_id, code, name, created_by_account_id, updated_by_account_id
                )
                VALUES ($1, $2, $3, $4, $5, $6, $6)
                ON CONFLICT (tenant_id, branch_id, lower(code)) DO UPDATE
                SET name = EXCLUDED.name,
                    status = 'active',
                    updated_at = CURRENT_TIMESTAMP,
                    updated_by_account_id = EXCLUDED.updated_by_account_id
                RETURNING id
                "#,
                facility_id,
                tenant_id,
                branch_id,
                facility.code,
                facility.name,
                owner_account_id,
            )
            .fetch_one(transaction.connection())
            .await
            .map_err(io::Error::other)?;
            debug!(
                "Development facility ensured: tenant_slug={} branch_id={} facility_id={} facility_code={}",
                tenant.slug, branch_id, facility_id, facility.code
            );
        }
    }

    transaction.commit().await.map_err(io::Error::other)?;
    info!(
        "Development branch hierarchy committed: tenant_slug={} tenant_id={}",
        tenant.slug, tenant_id
    );
    Ok(())
}

fn require_development_environment() -> Result<(), io::Error> {
    match std::env::var("APP_ENV") {
        Ok(value) if value == "development" => Ok(()),
        _ => Err(io::Error::other(
            "refusing to seed sample data unless APP_ENV=development",
        )),
    }
}

async fn ensure_account(
    db: &DatabaseAdapter,
    tenant_id: Uuid,
    username: &str,
    email: &str,
    role: DevRole,
    audit_account_id: Option<Uuid>,
) -> Result<SeedAccount, io::Error> {
    let mut transaction: infra_postgres::TenantTransaction =
        db.begin_tenant(tenant_id).await.map_err(io::Error::other)?;
    let existing: Option<ExistingDevAccountRow> = sqlx::query_as!(
        ExistingDevAccountRow,
        r#"
        SELECT id, email, primary_role_code
        FROM accounts
        WHERE tenant_id = $1 AND lower(username) = lower($2)
        "#,
        tenant_id,
        username,
    )
    .fetch_optional(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    let account_id: Uuid = if let Some(account) = existing {
        if account.primary_role_code != role.as_code() {
            return Err(io::Error::other(format!(
                "existing development account '{username}' has role {}, expected {}",
                account.primary_role_code,
                role.as_code()
            )));
        }
        if account.email.as_deref() != Some(email) {
            return Err(io::Error::other(format!(
                "existing development account '{username}' has a different email than '{email}'"
            )));
        }
        account.id
    } else {
        let new_account_id: Uuid = Uuid::new_v4();
        let inserted: SeedIdRow = sqlx::query_as!(
            SeedIdRow,
            r#"
            INSERT INTO accounts (
                id, tenant_id, username, email, status, primary_role_code,
                created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, 'active', $5, $6, $6)
            RETURNING id
            "#,
            new_account_id,
            tenant_id,
            username,
            email,
            role.as_code(),
            audit_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(io::Error::other)?;
        inserted.id
    };

    let role_insert: sqlx::postgres::PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO account_roles (tenant_id, account_id, role_code, assigned_by_account_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (tenant_id, account_id, role_code) DO NOTHING
        "#,
        tenant_id,
        account_id,
        role.as_code(),
        audit_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    trace!(
        tenant_id = %tenant_id,
        account_id = %account_id,
        rows_affected = role_insert.rows_affected(),
        "Development account role ensured"
    );
    transaction.commit().await.map_err(io::Error::other)?;

    Ok(SeedAccount {
        id: account_id,
        username: username.to_owned(),
        role,
    })
}

async fn ensure_identity(
    db: &DatabaseAdapter,
    issuer: &str,
    subject: &str,
    tenant_id: Uuid,
    account_id: Uuid,
) -> Result<(), io::Error> {
    sqlx::query!(
        r#"
        INSERT INTO account_identities (issuer, subject, tenant_id, account_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (tenant_id, account_id) DO UPDATE
        SET issuer = EXCLUDED.issuer, subject = EXCLUDED.subject
        "#,
        issuer,
        subject,
        tenant_id,
        account_id,
    )
    .execute(db.global_pool())
    .await
    .map_err(io::Error::other)?;
    Ok(())
}
