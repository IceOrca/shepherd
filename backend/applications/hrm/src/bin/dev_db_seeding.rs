use std::{collections::HashMap, error::Error, io, sync::Arc};

use chrono::{NaiveDate, NaiveTime};
use foundation_kernel::debug::*;
use foundation_auth::{
    AuthMngtEntity, AuthenticateUserError, CreateAccountError,
    account::{Role, UserAccount},
};
use foundation_auth::AuthProvider;
use foundation_postgres::DatabaseAdapter;
use uuid::Uuid;

const ACME_TENANT_ID: &str = "00000000-0000-4000-8000-000000000001";
const ACME1_TENANT_ID: &str = "00000000-0000-4000-8000-000000000002";
const ACME2_TENANT_ID: &str = "00000000-0000-4000-8000-000000000003";
const DEFAULT_DEV_PASSWORD: &str = "shepherd-dev-password";
const DEV_ATTENDANCE_ID_NAMESPACE: u128 = 0xd3a7_7e00_0000_4000_8000_0000_0000_0000;

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
    role: Role,
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
        username: "owner",
        role: Role::TenantOwner,
    },
    DevAccount {
        username: "supervisor",
        role: Role::Supervisor,
    },
    DevAccount {
        username: "employee",
        role: Role::Employee,
    },
];

const ACME1_ACCOUNTS: &[DevAccount] = &[
    DevAccount {
        username: "acme1Owner",
        role: Role::TenantOwner,
    },
    DevAccount {
        username: "acme1Supervisor1",
        role: Role::Supervisor,
    },
    DevAccount {
        username: "acme1Supervisor2",
        role: Role::Supervisor,
    },
    DevAccount {
        username: "acme1Employee1",
        role: Role::Employee,
    },
    DevAccount {
        username: "acme1Employee2",
        role: Role::Employee,
    },
];

const ACME2_ACCOUNTS: &[DevAccount] = &[
    DevAccount {
        username: "acme2Owner",
        role: Role::TenantOwner,
    },
    DevAccount {
        username: "acme2Supervisor1",
        role: Role::Supervisor,
    },
    DevAccount {
        username: "acme2Supervisor2",
        role: Role::Supervisor,
    },
    DevAccount {
        username: "acme2Employee1",
        role: Role::Employee,
    },
    DevAccount {
        username: "acme2Employee2",
        role: Role::Employee,
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
    log_notice!("Development seed started: configured_tenants={}", DEV_TENANTS.len());
    require_development_environment()?;
    log_debug!("Development environment guard accepted APP_ENV=development");

    log_info!("Connecting to PostgreSQL; migrations must already be applied with the SQLx CLI");
    let database: Arc<DatabaseAdapter> = DatabaseAdapter::new_arc().await;
    let repository = AuthProvider::new_arc(Arc::clone(&database));
    let authentication = AuthMngtEntity::new_arc(repository).await;
    let password: String = std::env::var("DEV_SEED_PASSWORD").unwrap_or_else(|_| DEFAULT_DEV_PASSWORD.to_owned());
    let password_source: &str = if std::env::var_os("DEV_SEED_PASSWORD").is_some() {
        "DEV_SEED_PASSWORD"
    } else {
        "built-in development default"
    };
    log_info!("Development account password selected: source={password_source}; value is not logged");

    for tenant in DEV_TENANTS {
        if let Err(error) = seed_tenant(&database, &authentication, tenant, &password).await {
            log_error!(
                "Development tenant seed failed: tenant_slug={} tenant_id={} error={}",
                tenant.slug,
                tenant.id,
                error
            );
            return Err(error.into());
        }
    }

    let account_count: usize = DEV_TENANTS.iter().map(|tenant: &DevTenant| tenant.accounts.len()).sum();
    log_notice!(
        "Development seed completed successfully: tenants={} accounts={}",
        DEV_TENANTS.len(),
        account_count
    );
    println!("Development data seeded successfully");
    println!("tenant_slugs=acme,acme1,acme2");
    println!("accounts_created_or_verified={account_count}");
    println!("password_source={password_source}");
    if password_source == "built-in development default" {
        println!("default_password={DEFAULT_DEV_PASSWORD}");
    }
    Ok(())
}

async fn seed_tenant(
    database: &Arc<DatabaseAdapter>,
    authentication: &Arc<AuthMngtEntity>,
    tenant: &DevTenant,
    password: &str,
) -> Result<(), io::Error> {
    let tenant_id: Uuid = Uuid::parse_str(tenant.id).map_err(io::Error::other)?;
    log_notice!(
        "Provisioning development tenant: tenant_slug={} tenant_id={} display_name={} expected_accounts={}",
        tenant.slug,
        tenant_id,
        tenant.display_name,
        tenant.accounts.len()
    );
    database
        .provision_tenant(tenant_id, tenant.slug, tenant.display_name)
        .await
        .map_err(io::Error::other)?;
    log_info!(
        "Tenant provisioned in shared tenant registry: tenant_slug={} tenant_id={}",
        tenant.slug,
        tenant_id
    );

    let owner_definition: &DevAccount = tenant
        .accounts
        .iter()
        .find(|account: &&DevAccount| account.role == Role::TenantOwner)
        .ok_or_else(|| io::Error::other(format!("tenant '{}' has no owner seed definition", tenant.slug)))?;
    let owner: UserAccount = ensure_account(
        authentication,
        tenant_id,
        owner_definition.username,
        password,
        owner_definition.role.clone(),
        None,
    )
    .await?;

    let mut seeded_accounts: Vec<UserAccount> = vec![owner.clone()];
    for account_definition in tenant
        .accounts
        .iter()
        .filter(|account: &&DevAccount| account.role != Role::TenantOwner)
    {
        let account: UserAccount = ensure_account(
            authentication,
            tenant_id,
            account_definition.username,
            password,
            account_definition.role.clone(),
            Some(owner.id),
        )
        .await?;
        seeded_accounts.push(account);
    }

    seed_branches_and_facilities(database, tenant_id, tenant, owner.id).await?;
    seed_hr_foundation(database, tenant_id, tenant, &seeded_accounts, owner.id).await?;

    log_notice!(
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

async fn seed_hr_foundation(
    database: &DatabaseAdapter,
    tenant_id: Uuid,
    tenant: &DevTenant,
    accounts: &[UserAccount],
    owner_account_id: Uuid,
) -> Result<(), io::Error> {
    let mut transaction = database.begin_tenant(tenant_id).await.map_err(io::Error::other)?;
    let effective_date: NaiveDate =
        NaiveDate::from_ymd_opt(2026, 1, 1).ok_or_else(|| io::Error::other("invalid HR seed effective date"))?;
    log_info!(
        "Seeding Odoo-inspired HR foundation: tenant_slug={} tenant_id={} employees={} effective_date={}",
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
        log_debug!(
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
        .filter(|account: &&UserAccount| account.role == Role::Supervisor)
        .filter_map(|account: &UserAccount| employee_ids.get(&account.id).copied())
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
            Role::TenantOwner => (
                head_office_branch_id,
                head_office_facility_id,
                administration_department_id,
                owner_job_id,
                None,
            ),
            Role::Supervisor => {
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
            Role::Employee => {
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
        log_debug!(
            "Development HR assignment ensured: tenant_slug={} employee_id={} assignment_id={} branch_id={} facility_id={} manager_employee_id={:?}",
            tenant.slug,
            employee_id,
            assignment_id,
            branch_id,
            facility_id,
            manager_employee_id
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
        log_debug!(
            "Development working schedule assignment ensured: tenant_slug={} employee_id={} schedule_id={} assignment_id={} effective_date={}",
            tenant.slug,
            employee_id,
            standard_schedule_id,
            schedule_assignment_id,
            effective_date
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
    log_notice!(
        "Development HR foundation committed: tenant_slug={} tenant_id={} departments=2 jobs=3 employees={} assignments={} working_schedules=1 schedule_assignments={} completed_attendance_sessions={} open_attendance_sessions={}",
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
    transaction: &mut foundation_postgres::TenantTransaction,
    tenant_id: Uuid,
    tenant: &DevTenant,
    accounts: &[UserAccount],
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
            Role::Employee => {
                sqlx::query!(
                    r#"
                    INSERT INTO hr_employee_compensations (
                        id, tenant_id, employee_id, currency, pay_basis, hourly_rate,
                        effective_from, created_by_account_id
                    )
                    VALUES ($1, $2, $3, 'THB', 'hourly', 120, $4, $5)
                    ON CONFLICT (tenant_id, employee_id, effective_from) DO UPDATE
                    SET currency = 'THB',
                        pay_basis = 'hourly',
                        hourly_rate = 120,
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
            Role::Supervisor | Role::TenantOwner => {
                let monthly_rate: &str = if account.role == Role::TenantOwner {
                    "50000"
                } else {
                    "30000"
                };
                sqlx::query!(
                    r#"
                    INSERT INTO hr_employee_compensations (
                        id, tenant_id, employee_id, currency, pay_basis, monthly_rate,
                        standard_monthly_hours, effective_from, created_by_account_id
                    )
                    VALUES ($1, $2, $3, 'THB', 'monthly', $4::TEXT::NUMERIC, 160, $5, $6)
                    ON CONFLICT (tenant_id, employee_id, effective_from) DO UPDATE
                    SET currency = 'THB',
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

    log_info!(
        "Development payroll configuration ensured: tenant_slug={} tenant_id={} compensations={} warehouse_rules={} time_rules=1 overtime_rules=2 currency=THB",
        tenant.slug,
        tenant_id,
        accounts.len(),
        warehouse_rule_count
    );
    Ok(())
}

async fn seed_attendance_sessions(
    transaction: &mut foundation_postgres::TenantTransaction,
    tenant_id: Uuid,
    tenant: &DevTenant,
    accounts: &[UserAccount],
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
        .filter(|account: &&UserAccount| account.role == Role::Employee)
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
                log_info!(
                    "Open attendance fixture skipped because another session is already open: tenant_slug={} employee_id={} username={}",
                    tenant.slug,
                    employee_id,
                    account.username
                );
            }
        }
    }

    log_info!(
        "Development attendance ensured: tenant_slug={} tenant_id={} completed_sessions={} open_sessions={}",
        tenant.slug,
        tenant_id,
        completed_count,
        open_count
    );
    Ok((completed_count, open_count))
}

#[allow(clippy::too_many_arguments)]
async fn ensure_completed_attendance_session(
    transaction: &mut foundation_postgres::TenantTransaction,
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
    transaction: &mut foundation_postgres::TenantTransaction,
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
    transaction: &mut foundation_postgres::TenantTransaction,
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
    database: &DatabaseAdapter,
    tenant_id: Uuid,
    tenant: &DevTenant,
    owner_account_id: Uuid,
) -> Result<(), io::Error> {
    let mut transaction = database.begin_tenant(tenant_id).await.map_err(io::Error::other)?;
    log_info!(
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
        log_debug!(
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
            log_debug!(
                "Development facility ensured: tenant_slug={} branch_id={} facility_id={} facility_code={}",
                tenant.slug,
                branch_id,
                facility_id,
                facility.code
            );
        }
    }

    transaction.commit().await.map_err(io::Error::other)?;
    log_notice!(
        "Development branch hierarchy committed: tenant_slug={} tenant_id={}",
        tenant.slug,
        tenant_id
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
    authentication: &AuthMngtEntity,
    tenant_id: Uuid,
    username: &str,
    password: &str,
    role: Role,
    audit_account_id: Option<Uuid>,
) -> Result<UserAccount, io::Error> {
    log_debug!(
        "Looking up development account: tenant_id={} username={} expected_role={}",
        tenant_id,
        username,
        role.as_code()
    );
    let account = if let Some(account) = authentication
        .get_current_account_by_username(tenant_id, username)
        .await
        .map_err(io::Error::other)?
    {
        log_info!(
            "Reusing existing development account: tenant_id={} account_id={} username={} role={}",
            tenant_id,
            account.id,
            username,
            account.role.as_code()
        );
        account
    } else {
        log_info!(
            "Creating development account: tenant_id={} username={} role={} audit_account_id={:?}",
            tenant_id,
            username,
            role.as_code(),
            audit_account_id
        );
        let created_account: UserAccount = authentication
            .create_account(tenant_id, username, password, role.clone(), audit_account_id)
            .await
            .map_err(|error: CreateAccountError| io::Error::other(format!("failed to seed '{username}': {error:?}")))?;
        log_notice!(
            "Development account created: tenant_id={} account_id={} username={} role={}",
            tenant_id,
            created_account.id,
            username,
            created_account.role.as_code()
        );
        created_account
    };

    if account.role != role {
        log_error!(
            "Development account role mismatch: tenant_id={} account_id={} username={} actual_role={} expected_role={}",
            tenant_id,
            account.id,
            username,
            account.role.as_code(),
            role.as_code()
        );
        return Err(io::Error::other(format!(
            "existing development account '{username}' has role {:?}, expected {role:?}",
            account.role
        )));
    }

    log_debug!(
        "Verifying development account credentials: tenant_id={} account_id={} username={} role={}",
        tenant_id,
        account.id,
        username,
        account.role.as_code()
    );
    let authenticated_account: UserAccount = authentication
        .authenticate_user(tenant_id, username, password)
        .await
        .map_err(|error: AuthenticateUserError| {
            io::Error::other(format!(
                "development account '{username}' does not accept DEV_SEED_PASSWORD: {error:?}"
            ))
        })?;
    log_info!(
        "Development account verified: tenant_id={} account_id={} username={} role={} active={}",
        tenant_id,
        authenticated_account.id,
        username,
        authenticated_account.role.as_code(),
        authenticated_account.active
    );
    Ok(authenticated_account)
}
