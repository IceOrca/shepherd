#![cfg_attr(debug_assertions, allow(unused))]

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs, io,
    path::Path,
    sync::Arc,
};

use chrono::{NaiveDate, NaiveTime};
use tracing::{error, warn, info, debug, trace};
use infra_kernel::debug::Debugging;
use infra_postgres::DatabaseAdapter;
use uuid::Uuid;

const DEFAULT_DEV_ACCOUNT_CATALOG: &str = "/run/config/dev-auth-accounts.tsv";
const DEFAULT_DEV_CUSTOMER_CATALOG: &str = "/run/config/dev-customers.tsv";
const DEV_ATTENDANCE_ID_NAMESPACE: u128 = 0xd3a7_7e00_0000_4000_8000_0000_0000_0000;
const DEV_STAFFING_SHIFT_ID_NAMESPACE: u128 = 0x51f7_0000_0000_4000_8000_0000_0000_0000;
const DEV_STAFFING_ASSIGNMENT_ID_NAMESPACE: u128 = 0xa551_0000_0000_4000_8000_0000_0000_0000;
const DEV_COMPLETED_SHIFT_ID_NAMESPACE: u128 = 0x61f7_0000_0000_4000_8000_0000_0000_0000;
const DEV_COMPLETED_ASSIGNMENT_ID_NAMESPACE: u128 = 0x6a55_0000_0000_4000_8000_0000_0000_0000;
const DEV_COMPLETED_SESSION_ID_NAMESPACE: u128 = 0x6e55_0000_0000_4000_8000_0000_0000_0000;
const DEV_COMPLETED_CUSTOMER_RECORD_ID_NAMESPACE: u128 = 0x6c55_0000_0000_4000_8000_0000_0000_0000;
const DEV_URGENT_BATCH_ID_NAMESPACE: u128 = 0xb47c_0000_0000_4000_8000_0000_0000_0000;
const DEV_URGENT_REPORT_ID_NAMESPACE: u128 = 0xc47c_0000_0000_4000_8000_0000_0000_0000;
const DEV_URGENT_SESSION_ID_NAMESPACE: u128 = 0xd47c_0000_0000_4000_8000_0000_0000_0000;
const DEV_URGENT_CUSTOMER_RECORD_ID_NAMESPACE: u128 = 0xe47c_0000_0000_4000_8000_0000_0000_0000;
const DEV_COMPANY_EXPENSE_ID_NAMESPACE: u128 = 0xf100_0000_0000_4000_8000_0000_0000_0000;
const DEV_PERSONAL_EXPENSE_ID_NAMESPACE: u128 = 0xf200_0000_0000_4000_8000_0000_0000_0000;
const DEV_EXPENSE_REIMBURSEMENT_ID_NAMESPACE: u128 = 0xf300_0000_0000_4000_8000_0000_0000_0000;
const DEV_SALARY_ADVANCE_ID_NAMESPACE: u128 = 0xf400_0000_0000_4000_8000_0000_0000_0000;
const DEV_SALARY_RECOVERY_ID_NAMESPACE: u128 = 0xf500_0000_0000_4000_8000_0000_0000_0000;

struct SeedIdRow {
    id: Uuid,
}

struct ExistingDevAccountRow {
    id: Uuid,
    email: Option<String>,
    primary_role_code: String,
}

struct DevTenant {
    id: Uuid,
    slug: String,
    display_name: String,
    accounts: Vec<DevAccount>,
}

struct DevBranch {
    code: &'static str,
    name: &'static str,
    time_zone: &'static str,
}

#[derive(Clone, Debug)]
struct DevCustomer {
    branch_code: String,
    code: String,
    name: String,
    address: String,
    time_zone: String,
}

struct DevAccount {
    username: String,
    email: String,
    role: DevRole,
    branch_code: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DevRole {
    TenantOwner,
    ExecutiveManager,
    BranchManager,
    Supervisor,
    Staff,
}

impl DevRole {
    const fn as_code(self) -> &'static str {
        match self {
            Self::TenantOwner => "tenant_owner",
            Self::ExecutiveManager => "executive_manager",
            Self::BranchManager => "branch_manager",
            Self::Supervisor => "supervisor",
            Self::Staff => "staff",
        }
    }

    fn parse(value: &str) -> Result<Self, io::Error> {
        match value {
            "tenant_owner" => Ok(Self::TenantOwner),
            "executive_manager" => Ok(Self::ExecutiveManager),
            "branch_manager" => Ok(Self::BranchManager),
            "supervisor" => Ok(Self::Supervisor),
            "staff" => Ok(Self::Staff),
            unsupported => Err(io::Error::other(format!(
                "unsupported development role '{unsupported}'"
            ))),
        }
    }
}

#[derive(Clone, Debug)]
struct SeedAccount {
    id: Uuid,
    username: String,
    role: DevRole,
    branch_code: Option<String>,
}

const DEV_BRANCHES: &[DevBranch] = &[
    DevBranch {
        code: "head-office",
        name: "Trụ sở chính",
        time_zone: "Asia/Ho_Chi_Minh",
    },
    DevBranch {
        code: "north-branch",
        name: "Chi nhánh miền Bắc",
        time_zone: "Asia/Ho_Chi_Minh",
    },
    DevBranch {
        code: "south-branch",
        name: "Chi nhánh miền Trung",
        time_zone: "Asia/Ho_Chi_Minh",
    },
];
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Debugging::init();
    require_development_environment()?;
    let catalog_path: String =
        std::env::var("DEV_AUTH_ACCOUNTS_FILE").unwrap_or_else(|_| DEFAULT_DEV_ACCOUNT_CATALOG.to_owned());
    let dev_tenants: Vec<DevTenant> = load_dev_tenants(Path::new(&catalog_path))?;
    let customer_catalog_path: String =
        std::env::var("DEV_CUSTOMERS_FILE").unwrap_or_else(|_| DEFAULT_DEV_CUSTOMER_CATALOG.to_owned());
    let dev_customers: Vec<DevCustomer> = load_dev_customers(Path::new(&customer_catalog_path))?;
    info!(
        configured_tenants = dev_tenants.len(),
        configured_customers_per_tenant = dev_customers.len(),
        catalog_path,
        customer_catalog_path,
        "Development seed started from external catalogs"
    );
    let auth_issuer: String = std::env::var("AUTH_ISSUER_URL")
        .map_err(|_| io::Error::other("AUTH_ISSUER_URL is required for development seeding"))?;
    let auth_identities_json: String = std::env::var("AUTH_DEV_IDENTITIES_JSON")
        .map_err(|_| io::Error::other("AUTH_DEV_IDENTITIES_JSON is required for development seeding"))?;
    let auth_identities: HashMap<String, Uuid> = serde_json::from_str(&auth_identities_json)
        .map_err(|error: serde_json::Error| io::Error::other(format!("parse Auth identity map: {error}")))?;
    let expected_account_count: usize = dev_tenants
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
    for tenant in &dev_tenants {
        if let Err(error) = seed_tenant(&db, tenant, &dev_customers, &auth_issuer, &auth_identities).await {
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
        dev_tenants.len(),
        account_count
    );
    println!("Development data seeded successfully");
    let tenant_slugs: String = dev_tenants
        .iter()
        .map(|tenant: &DevTenant| tenant.slug.as_str())
        .collect::<Vec<&str>>()
        .join(",");
    println!("tenant_slugs={tenant_slugs}");
    println!("accounts_created_or_verified={account_count}");
    println!("auth_identity_issuer={auth_issuer}");
    println!("auth_identity_linked=true");
    Ok(())
}

fn load_dev_customers(path: &Path) -> Result<Vec<DevCustomer>, io::Error> {
    let contents: String = fs::read_to_string(path)
        .map_err(|error: io::Error| io::Error::other(format!("read {}: {error}", path.display())))?;
    let known_branches: HashSet<&str> = DEV_BRANCHES.iter().map(|branch: &DevBranch| branch.code).collect();
    let mut customers: Vec<DevCustomer> = Vec::new();
    let mut seen_scopes: HashSet<(String, String)> = HashSet::new();

    for (line_index, raw_line) in contents.lines().enumerate() {
        let line_number: usize = line_index + 1;
        if raw_line.trim().is_empty() || raw_line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = raw_line.split('\t').collect();
        let [branch_code_raw, code_raw, name_raw, address_raw, time_zone_raw] = columns.as_slice() else {
            return Err(io::Error::other(format!(
                "{}:{line_number} must contain 5 tab-separated columns",
                path.display()
            )));
        };
        let branch_code: String = branch_code_raw.trim().to_owned();
        let code: String = code_raw.trim().to_owned();
        let name: String = name_raw.trim().to_owned();
        let address: String = address_raw.trim().to_owned();
        let time_zone: String = time_zone_raw.trim().to_owned();
        if branch_code.is_empty() || code.is_empty() || name.is_empty() || address.is_empty() || time_zone.is_empty() {
            return Err(io::Error::other(format!(
                "{}:{line_number} contains a blank required value",
                path.display()
            )));
        }
        if !known_branches.contains(branch_code.as_str()) {
            return Err(io::Error::other(format!(
                "{}:{line_number} references unknown branch code '{branch_code}'",
                path.display()
            )));
        }
        if !seen_scopes.insert((branch_code.clone(), code.to_lowercase())) {
            return Err(io::Error::other(format!(
                "{}:{line_number} duplicates customer code '{code}' in branch '{branch_code}'",
                path.display()
            )));
        }
        customers.push(DevCustomer {
            branch_code,
            code,
            name,
            address,
            time_zone,
        });
    }

    for branch in DEV_BRANCHES {
        let customer_count: usize = customers
            .iter()
            .filter(|customer: &&DevCustomer| customer.branch_code == branch.code)
            .count();
        if customer_count != 3 {
            return Err(io::Error::other(format!(
                "development branch '{}' must contain exactly 3 customers, found {customer_count}",
                branch.code
            )));
        }
    }
    Ok(customers)
}

fn load_dev_tenants(path: &Path) -> Result<Vec<DevTenant>, io::Error> {
    let contents: String = fs::read_to_string(path)
        .map_err(|error: io::Error| io::Error::other(format!("read {}: {error}", path.display())))?;
    let mut tenants: Vec<DevTenant> = Vec::new();
    let mut seen_emails: HashSet<String> = HashSet::new();
    for (line_index, raw_line) in contents.lines().enumerate() {
        let line_number: usize = line_index + 1;
        if raw_line.trim().is_empty() || raw_line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = raw_line.split('\t').collect();
        let [
            tenant_id_raw,
            tenant_slug_raw,
            display_name_raw,
            role_raw,
            username_raw,
            email_raw,
            password,
            branch_raw,
        ] = columns.as_slice()
        else {
            return Err(io::Error::other(format!(
                "{}:{line_number} must contain 8 tab-separated columns",
                path.display()
            )));
        };
        let tenant_id: Uuid = Uuid::parse_str(tenant_id_raw).map_err(|error: uuid::Error| {
            io::Error::other(format!(
                "{}:{line_number} has invalid tenant UUID: {error}",
                path.display()
            ))
        })?;
        let tenant_slug: String = tenant_slug_raw.trim().to_owned();
        let display_name: String = display_name_raw.trim().to_owned();
        let role: DevRole = DevRole::parse(role_raw.trim())?;
        let username: String = username_raw.trim().to_owned();
        let email: String = email_raw.trim().to_lowercase();
        let raw_branch: &str = branch_raw.trim();
        if tenant_slug.is_empty()
            || display_name.is_empty()
            || username.is_empty()
            || email.is_empty()
            || password.is_empty()
        {
            return Err(io::Error::other(format!(
                "{}:{line_number} contains a blank required value",
                path.display()
            )));
        }
        if !seen_emails.insert(email.clone()) {
            return Err(io::Error::other(format!(
                "{}:{line_number} reuses email '{email}'; development identities are single-tenant",
                path.display()
            )));
        }
        let branch_code: Option<String> = match role {
            DevRole::TenantOwner | DevRole::ExecutiveManager => {
                if raw_branch != "all" {
                    return Err(io::Error::other(format!(
                        "{}:{line_number} tenant-wide role branch must be 'all'",
                        path.display()
                    )));
                }
                None
            }
            DevRole::BranchManager | DevRole::Supervisor | DevRole::Staff => {
                if raw_branch.is_empty() || raw_branch == "all" {
                    return Err(io::Error::other(format!(
                        "{}:{line_number} branch-scoped role requires one branch code",
                        path.display()
                    )));
                }
                Some(raw_branch.to_owned())
            }
        };
        let account: DevAccount = DevAccount {
            username,
            email,
            role,
            branch_code,
        };
        match tenants
            .iter_mut()
            .find(|tenant: &&mut DevTenant| tenant.slug == tenant_slug)
        {
            Some(tenant) => {
                if tenant.id != tenant_id || tenant.display_name != display_name {
                    return Err(io::Error::other(format!(
                        "{}:{line_number} conflicts with earlier metadata for tenant '{tenant_slug}'",
                        path.display()
                    )));
                }
                tenant.accounts.push(account);
            }
            None => tenants.push(DevTenant {
                id: tenant_id,
                slug: tenant_slug,
                display_name,
                accounts: vec![account],
            }),
        }
    }
    if tenants.is_empty() {
        return Err(io::Error::other(format!(
            "{} contains no development accounts",
            path.display()
        )));
    }
    for tenant in &tenants {
        let owner_count: usize = tenant
            .accounts
            .iter()
            .filter(|account: &&DevAccount| account.role == DevRole::TenantOwner)
            .count();
        if owner_count != 1 {
            return Err(io::Error::other(format!(
                "tenant '{}' must contain exactly one development tenant owner, found {owner_count}",
                tenant.slug
            )));
        }
    }
    Ok(tenants)
}

async fn seed_tenant(
    db: &Arc<DatabaseAdapter>,
    tenant: &DevTenant,
    dev_customers: &[DevCustomer],
    auth_issuer: &str,
    auth_identities: &HashMap<String, Uuid>,
) -> Result<(), io::Error> {
    let tenant_id: Uuid = tenant.id;
    info!(
        "Provisioning development tenant: tenant_slug={} tenant_id={} display_name={} expected_accounts={}",
        tenant.slug,
        tenant_id,
        tenant.display_name,
        tenant.accounts.len()
    );
    db.provision_tenant(tenant_id, &tenant.slug, &tenant.display_name)
        .await
        .map_err(io::Error::other)?;
    info!(
        "Tenant provisioned in shared tenant registry: tenant_slug={} tenant_id={}",
        tenant.slug, tenant_id
    );

    let owner_definition: &DevAccount = tenant
        .accounts
        .iter()
        .find(|account: &&DevAccount| account.role == DevRole::TenantOwner)
        .ok_or_else(|| io::Error::other(format!("tenant '{}' has no owner seed definition", tenant.slug)))?;
    let owner: SeedAccount = ensure_account(
        db,
        tenant_id,
        &owner_definition.username,
        &owner_definition.email,
        owner_definition.role,
        owner_definition.branch_code.as_deref(),
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
        .filter(|account: &&DevAccount| account.role != DevRole::TenantOwner)
    {
        let account: SeedAccount = ensure_account(
            db,
            tenant_id,
            &account_definition.username,
            &account_definition.email,
            account_definition.role,
            account_definition.branch_code.as_deref(),
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

    seed_branches(db, tenant_id, tenant, &seeded_accounts, owner.id).await?;
    seed_hr_infra(db, tenant_id, tenant, &seeded_accounts, owner.id).await?;
    seed_staffing_business(db, tenant_id, tenant, dev_customers, owner.id).await?;
    seed_financial_workflows(db, tenant_id, tenant, owner.id).await?;

    info!(
        "Development tenant seed completed: tenant_slug={} tenant_id={} accounts={} branches={} employees={}",
        tenant.slug,
        tenant_id,
        tenant.accounts.len(),
        DEV_BRANCHES.len(),
        seeded_accounts
            .iter()
            .filter(|account: &&SeedAccount| account.role != DevRole::TenantOwner)
            .count()
    );
    Ok(())
}

async fn seed_financial_workflows(
    db: &DatabaseAdapter,
    tenant_id: Uuid,
    tenant: &DevTenant,
    owner_account_id: Uuid,
) -> Result<(), io::Error> {
    let mut transaction = db.begin_tenant(tenant_id).await.map_err(io::Error::other)?;
    let branch_id: Uuid = sqlx::query_scalar!(
        "SELECT id FROM branches WHERE tenant_id = $1 AND code = 'head-office'",
        tenant_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO business_expense_categories (tenant_id, code, display_name)
        VALUES
            ($1, 'di_chuyen', 'Đi lại và vận chuyển'),
            ($1, 'vat_tu', 'Vật tư và đồ dùng'),
            ($1, 'tiep_khach', 'Tiếp khách'),
            ($1, 'xu_ly_khan_cap', 'Xử lý tình huống khẩn cấp'),
            ($1, 'khac', 'Chi phí khác')
        ON CONFLICT (tenant_id, code) DO NOTHING
        "#,
        tenant_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let emergency_category_id: Uuid = sqlx::query_scalar!(
        "SELECT id FROM business_expense_categories WHERE tenant_id = $1 AND code = 'xu_ly_khan_cap'",
        tenant_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let supplies_category_id: Uuid = sqlx::query_scalar!(
        "SELECT id FROM business_expense_categories WHERE tenant_id = $1 AND code = 'vat_tu'",
        tenant_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let manager = sqlx::query!(
        r#"
        SELECT employee.id AS employee_id, employee.account_id AS "account_id!"
        FROM hr_employees AS employee
        JOIN accounts AS account ON account.tenant_id = employee.tenant_id AND account.id = employee.account_id
        WHERE employee.tenant_id = $1 AND employee.branch_id = $2
          AND employee.status = 'active' AND account.primary_role_code = 'branch_manager'
        ORDER BY employee.employee_code LIMIT 1
        "#,
        tenant_id,
        branch_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let supervisor = sqlx::query!(
        r#"
        SELECT employee.id AS employee_id, employee.account_id AS "account_id!"
        FROM hr_employees AS employee
        JOIN accounts AS account ON account.tenant_id = employee.tenant_id AND account.id = employee.account_id
        WHERE employee.tenant_id = $1 AND employee.branch_id = $2
          AND employee.status = 'active' AND account.primary_role_code = 'supervisor'
        ORDER BY employee.employee_code LIMIT 1
        "#,
        tenant_id,
        branch_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let staff = sqlx::query!(
        r#"
        SELECT employee.id AS employee_id, employee.account_id AS "account_id!"
        FROM hr_employees AS employee
        JOIN accounts AS account ON account.tenant_id = employee.tenant_id AND account.id = employee.account_id
        WHERE employee.tenant_id = $1 AND employee.branch_id = $2
          AND employee.status = 'active' AND account.primary_role_code = 'staff'
        ORDER BY employee.employee_code LIMIT 1
        "#,
        tenant_id,
        branch_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    let company_expense_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_COMPANY_EXPENSE_ID_NAMESPACE);
    let personal_expense_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_PERSONAL_EXPENSE_ID_NAMESPACE);
    let reimbursement_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_EXPENSE_REIMBURSEMENT_ID_NAMESPACE);
    let salary_advance_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_SALARY_ADVANCE_ID_NAMESPACE);
    let recovery_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_SALARY_RECOVERY_ID_NAMESPACE);

    sqlx::query!(
        r#"
        INSERT INTO business_expense_claims (
            id, tenant_id, branch_id, category_id, funding_source, paid_on,
            payroll_inclusion_on,
            description, evidence_reference, claimed_amount, approved_amount,
            currency, status, submitted_by_account_id, approved_by_account_id,
            approved_at, submission_idempotency_key
        ) VALUES (
            $1, $2, $3, $4, 'company_funds', CURRENT_DATE - 2, CURRENT_DATE,
            'Mua vật tư xử lý sự cố điện tại điểm khách hàng', 'HD-DEV-VATTU-001',
            450000, 450000, 'VND', 'approved', $5, $6, CURRENT_TIMESTAMP, $1
        ) ON CONFLICT (id) DO NOTHING
        "#,
        company_expense_id,
        tenant_id,
        branch_id,
        emergency_category_id,
        manager.account_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO business_expense_claims (
            id, tenant_id, branch_id, category_id, funding_source, paid_by_employee_id,
            paid_on, payroll_inclusion_on, description, evidence_reference,
            claimed_amount, approved_amount,
            currency, status, submitted_by_account_id, approved_by_account_id,
            approved_at, submission_idempotency_key
        ) VALUES (
            $1, $2, $3, $4, 'employee_personal', $5, CURRENT_DATE - 1, CURRENT_DATE,
            'Giám sát mua nước và đồ dùng bổ sung cho ca phát sinh', 'HD-DEV-CHIHO-001',
            320000, 320000, 'VND', 'approved', $6, $7, CURRENT_TIMESTAMP, $1
        ) ON CONFLICT (id) DO NOTHING
        "#,
        personal_expense_id,
        tenant_id,
        branch_id,
        supplies_category_id,
        supervisor.employee_id,
        supervisor.account_id,
        manager.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO business_expense_claim_events (
            tenant_id, branch_id, expense_claim_id, action, actor_account_id,
            idempotency_key
        ) VALUES
            ($1, $2, $3, 'submitted', $4, $3),
            ($1, $2, $3, 'approved', $5, $6),
            ($1, $2, $7, 'submitted', $8, $7),
            ($1, $2, $7, 'approved', $4, $9)
        ON CONFLICT (tenant_id, actor_account_id, idempotency_key) DO NOTHING
        "#,
        tenant_id,
        branch_id,
        company_expense_id,
        manager.account_id,
        owner_account_id,
        Uuid::from_u128(company_expense_id.as_u128() ^ 1),
        personal_expense_id,
        supervisor.account_id,
        Uuid::from_u128(personal_expense_id.as_u128() ^ 1),
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO business_expense_reimbursements (
            id, tenant_id, branch_id, expense_claim_id, employee_id, amount,
            currency, payment_reference, recorded_by_account_id, idempotency_key
        ) VALUES ($1, $2, $3, $4, $5, 100000, 'VND', 'CK-DEV-HOAN-001', $6, $1)
        ON CONFLICT (id) DO NOTHING
        "#,
        reimbursement_id,
        tenant_id,
        branch_id,
        personal_expense_id,
        supervisor.employee_id,
        manager.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO hr_salary_advances (
            id, tenant_id, branch_id, employee_id, requested_amount, approved_amount,
            currency, reason, paid_on, payroll_inclusion_on, status, requested_by_account_id,
            approved_by_account_id, disbursed_by_account_id, disbursement_reference,
            approved_at, disbursed_at, request_idempotency_key
        ) VALUES (
            $1, $2, $3, $4, 1000000, 1000000, 'VND',
            'Tạm ứng chi phí gia đình trong tháng', CURRENT_DATE,
            CURRENT_DATE + 20, 'disbursed',
            $5, $6, $6, 'CK-DEV-TAMUNG-001', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, $1
        ) ON CONFLICT (id) DO NOTHING
        "#,
        salary_advance_id,
        tenant_id,
        branch_id,
        staff.employee_id,
        staff.account_id,
        manager.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO hr_salary_advance_events (
            tenant_id, branch_id, salary_advance_id, action, actor_account_id,
            idempotency_key
        ) VALUES
            ($1, $2, $3, 'requested', $4, $3),
            ($1, $2, $3, 'approved', $5, $6),
            ($1, $2, $3, 'disbursed', $5, $7)
        ON CONFLICT (tenant_id, actor_account_id, idempotency_key) DO NOTHING
        "#,
        tenant_id,
        branch_id,
        salary_advance_id,
        staff.account_id,
        manager.account_id,
        Uuid::from_u128(salary_advance_id.as_u128() ^ 1),
        Uuid::from_u128(salary_advance_id.as_u128() ^ 2),
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO hr_salary_advance_recoveries (
            id, tenant_id, branch_id, salary_advance_id, employee_id, amount,
            currency, recovery_source, settlement_reference, recorded_by_account_id,
            idempotency_key
        ) VALUES (
            $1, $2, $3, $4, $5, 250000, 'VND', 'manual_repayment',
            'THU-HOI-DEV-001', $6, $1
        ) ON CONFLICT (id) DO NOTHING
        "#,
        recovery_id,
        tenant_id,
        branch_id,
        salary_advance_id,
        staff.employee_id,
        manager.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    sqlx::query!(
        r#"
        INSERT INTO hr_employee_salary_rates (
            id, tenant_id, branch_id, employee_id, monthly_amount, currency,
            effective_from, created_by_account_id, idempotency_key
        )
        SELECT gen_random_uuid(), employee.tenant_id, employee.branch_id, employee.id,
               CASE account.primary_role_code
                   WHEN 'executive_manager' THEN 30000000
                   WHEN 'branch_manager' THEN 22000000
                   WHEN 'supervisor' THEN 14000000
               END,
               'VND', DATE '2026-01-01', $2, gen_random_uuid()
        FROM hr_employees AS employee
        JOIN accounts AS account
          ON account.tenant_id = employee.tenant_id AND account.id = employee.account_id
        WHERE employee.tenant_id = $1
          AND employee.status = 'active'
          AND account.primary_role_code IN ('executive_manager', 'branch_manager', 'supervisor')
        ON CONFLICT (tenant_id, branch_id, employee_id, effective_from) DO NOTHING
        "#,
        tenant_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    transaction.commit().await.map_err(io::Error::other)?;
    info!(
        tenant_slug = tenant.slug,
        tenant_id = %tenant_id,
        company_expense_id = %company_expense_id,
        personal_expense_id = %personal_expense_id,
        salary_advance_id = %salary_advance_id,
        "Development financial workflows committed"
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
    dev_customers: &[DevCustomer],
    owner_account_id: Uuid,
) -> Result<(), io::Error> {
    let mut transaction = db.begin_tenant(tenant_id).await.map_err(io::Error::other)?;
    let effective_date =
        NaiveDate::from_ymd_opt(2026, 1, 1).ok_or_else(|| io::Error::other("invalid staffing seed date"))?;
    let branch_id: Uuid = sqlx::query_scalar!(
        "SELECT id FROM branches WHERE tenant_id = $1 AND code = 'head-office'",
        tenant_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let job: SeedIdRow = sqlx::query_as!(
        SeedIdRow,
        "SELECT id FROM business_staffing_jobs WHERE tenant_id = $1 AND branch_id = $2 AND code = 'employee'",
        tenant_id,
        branch_id,
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
        INNER JOIN accounts AS account
            ON account.tenant_id = employee.tenant_id
           AND account.id = employee.account_id
           AND account.primary_role_code = 'staff'
        WHERE employee.tenant_id = $1 AND employee.status = 'active'
          AND employee.branch_id = $2
        ORDER BY employee.employee_code
        LIMIT 1
        "#,
        tenant_id,
        branch_id,
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

    let branch_rows = sqlx::query!(
        "SELECT id, code FROM branches WHERE tenant_id = $1 AND status = 'active'",
        tenant_id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let branch_ids_by_code: HashMap<String, Uuid> =
        branch_rows.into_iter().map(|branch| (branch.code, branch.id)).collect();
    let mut customer_ids: Vec<(String, Uuid)> = Vec::with_capacity(dev_customers.len());
    for customer in dev_customers {
        let customer_branch_id: Uuid = branch_ids_by_code.get(&customer.branch_code).copied().ok_or_else(|| {
            io::Error::other(format!(
                "development customer '{}' references missing branch '{}'",
                customer.code, customer.branch_code
            ))
        })?;
        let customer_id: Uuid = ensure_staffing_customer(
            &mut transaction,
            tenant_id,
            customer_branch_id,
            &customer.code,
            &customer.name,
            &customer.address,
            &customer.time_zone,
            owner_account_id,
        )
        .await?;
        customer_ids.push((customer.branch_code.clone(), customer_id));
        ensure_staffing_rate(
            &mut transaction,
            tenant_id,
            customer_branch_id,
            &format!("{}-default", customer.code),
            &format!("Đơn giá mặc định cho {}", customer.name),
            customer_id,
            None,
            "150000.0000",
            "120000.0000",
            0,
            effective_date,
            owner_account_id,
        )
        .await?;
    }
    let head_customer_ids: Vec<Uuid> = customer_ids
        .iter()
        .filter_map(|(branch_code, customer_id): &(String, Uuid)| {
            (branch_code == "head-office").then_some(*customer_id)
        })
        .collect();
    let karaoke_a_id: Uuid = *head_customer_ids
        .first()
        .ok_or_else(|| io::Error::other("head office development customer was not found"))?;
    let karaoke_a: &DevCustomer = dev_customers
        .iter()
        .find(|customer: &&DevCustomer| customer.branch_code == "head-office")
        .ok_or_else(|| io::Error::other("head office development customer definition was not found"))?;
    let karaoke_b_id: Uuid = *head_customer_ids
        .get(1)
        .ok_or_else(|| io::Error::other("second head office development customer was not found"))?;

    let (customer_bill_rate_id, worker_pay_rate_id) = ensure_staffing_rate(
        &mut transaction,
        tenant_id,
        branch_id,
        "karaoke-b-worker-special",
        "Đơn giá riêng theo nhân viên",
        karaoke_b_id,
        Some(employee_id),
        "180000.0000",
        "145000.0000",
        100,
        effective_date,
        owner_account_id,
    )
    .await?;
    let shift_id = Uuid::from_u128(tenant_id.as_u128() ^ DEV_STAFFING_SHIFT_ID_NAMESPACE);
    let assignment_id = Uuid::from_u128(tenant_id.as_u128() ^ DEV_STAFFING_ASSIGNMENT_ID_NAMESPACE);
    sqlx::query!(
        r#"
        INSERT INTO business_staffing_shifts (
            id, tenant_id, branch_id, customer_id, job_id, starts_at, ends_at,
            required_workers, status, notes, created_by_account_id, updated_by_account_id
        )
        VALUES (
            $1, $2, $3, $4, $5, CURRENT_TIMESTAMP - INTERVAL '15 minutes',
            CURRENT_TIMESTAMP + INTERVAL '6 hours', 1, 'filled',
            'Ca làm việc mẫu để kiểm tra thao tác bắt đầu và kết thúc', $6, $6
        )
        ON CONFLICT (id) DO NOTHING
        "#,
        shift_id,
        tenant_id,
        branch_id,
        karaoke_b_id,
        job_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO business_shift_assignments (
            id, tenant_id, branch_id, shift_id, employee_id, customer_bill_rate_id, worker_pay_rate_id,
            rate_source, currency, bill_hourly_rate_snapshot, worker_hourly_rate_snapshot,
            created_by_account_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'configured', 'VND', 180000, 145000, $8)
        ON CONFLICT (id) DO NOTHING
        "#,
        assignment_id,
        tenant_id,
        branch_id,
        shift_id,
        employee_id,
        customer_bill_rate_id,
        worker_pay_rate_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;

    let historical_employee: SeedIdRow = sqlx::query_as!(
        SeedIdRow,
        r#"
        SELECT employee.id
        FROM hr_employees AS employee
        INNER JOIN accounts AS account
            ON account.tenant_id = employee.tenant_id
           AND account.id = employee.account_id
           AND account.primary_role_code = 'staff'
        WHERE employee.tenant_id = $1 AND employee.status = 'active'
          AND employee.branch_id = $2
        ORDER BY employee.employee_code
        OFFSET 1 LIMIT 1
        "#,
        tenant_id,
        branch_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let historical_employee_id: Uuid = historical_employee.id;
    let historical_staff_account_id: Uuid = sqlx::query_scalar!(
        r#"SELECT account_id AS "account_id!" FROM hr_employees WHERE tenant_id = $1 AND id = $2"#,
        tenant_id,
        historical_employee_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let historical_bill_rate_id: Uuid = sqlx::query_scalar!(
        r#"
        SELECT id FROM business_staffing_rates
        WHERE tenant_id = $1 AND branch_id = $2 AND customer_id = $3
          AND employee_id IS NULL AND rate_kind = 'customer_bill'
          AND code = $4 AND effective_from = $5
        "#,
        tenant_id,
        branch_id,
        karaoke_a_id,
        format!("{}-default-bill", karaoke_a.code),
        effective_date,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let historical_pay_rate_id: Uuid = sqlx::query_scalar!(
        r#"
        SELECT id FROM business_staffing_rates
        WHERE tenant_id = $1 AND branch_id = $2 AND customer_id = $3
          AND employee_id IS NULL AND rate_kind = 'worker_pay'
          AND code = $4 AND effective_from = $5
        "#,
        tenant_id,
        branch_id,
        karaoke_a_id,
        format!("{}-default-pay", karaoke_a.code),
        effective_date,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let completed_shift_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_COMPLETED_SHIFT_ID_NAMESPACE);
    let completed_assignment_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_COMPLETED_ASSIGNMENT_ID_NAMESPACE);
    let completed_session_id: Uuid = Uuid::from_u128(tenant_id.as_u128() ^ DEV_COMPLETED_SESSION_ID_NAMESPACE);
    let completed_customer_record_id: Uuid =
        Uuid::from_u128(tenant_id.as_u128() ^ DEV_COMPLETED_CUSTOMER_RECORD_ID_NAMESPACE);
    sqlx::query!(
        r#"
        INSERT INTO business_staffing_shifts (
            id, tenant_id, branch_id, customer_id, job_id, starts_at, ends_at,
            required_workers, status, notes, created_by_account_id, updated_by_account_id
        ) VALUES (
            $1, $2, $3, $4, $5,
            (((CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Ho_Chi_Minh')::DATE - 1) + TIME '18:00')
                AT TIME ZONE 'Asia/Ho_Chi_Minh',
            ((CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Ho_Chi_Minh')::DATE + TIME '00:00')
                AT TIME ZONE 'Asia/Ho_Chi_Minh',
            1, 'completed', 'Ca đã đối soát mẫu dùng cho báo cáo lương và tài chính', $6, $6
        )
        ON CONFLICT (id) DO NOTHING
        "#,
        completed_shift_id,
        tenant_id,
        branch_id,
        karaoke_a_id,
        job_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO business_shift_assignments (
            id, tenant_id, branch_id, shift_id, employee_id,
            customer_bill_rate_id, worker_pay_rate_id, rate_source, currency,
            bill_hourly_rate_snapshot, worker_hourly_rate_snapshot, status,
            worked_seconds, customer_amount, worker_amount, margin_amount,
            approved_at, approved_by_account_id, created_by_account_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, 'configured', 'VND',
            150000, 120000, 'approved', 21600, 900000, 720000, 180000,
            CURRENT_TIMESTAMP, $8, $8
        )
        ON CONFLICT (id) DO NOTHING
        "#,
        completed_assignment_id,
        tenant_id,
        branch_id,
        completed_shift_id,
        historical_employee_id,
        historical_bill_rate_id,
        historical_pay_rate_id,
        owner_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO business_shift_work_sessions (
            id, tenant_id, branch_id, assignment_id, employee_id,
            started_at, ended_at, start_idempotency_key, end_idempotency_key,
            started_by_account_id, ended_by_account_id
        ) VALUES (
            $1, $2, $3, $4, $5,
            (((CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Ho_Chi_Minh')::DATE - 1) + TIME '18:00')
                AT TIME ZONE 'Asia/Ho_Chi_Minh',
            ((CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Ho_Chi_Minh')::DATE + TIME '00:00')
                AT TIME ZONE 'Asia/Ho_Chi_Minh',
            $1, $6, $7, $7
        )
        ON CONFLICT (id) DO NOTHING
        "#,
        completed_session_id,
        tenant_id,
        branch_id,
        completed_assignment_id,
        historical_employee_id,
        completed_customer_record_id,
        historical_staff_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    sqlx::query!(
        r#"
        INSERT INTO business_customer_work_records (
            id, tenant_id, branch_id, assignment_id, confirmed_customer_id,
            confirmed_started_at, confirmed_ended_at, customer_reference,
            notes, recorded_by_account_id
        ) VALUES (
            $1, $2, $3, $4, $5,
            (((CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Ho_Chi_Minh')::DATE - 1) + TIME '18:00')
                AT TIME ZONE 'Asia/Ho_Chi_Minh',
            ((CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Ho_Chi_Minh')::DATE + TIME '00:00')
                AT TIME ZONE 'Asia/Ho_Chi_Minh',
            'DEV-DA-DOI-SOAT-001', 'Bằng chứng khách hàng mẫu đã được đối soát', $6
        )
        ON CONFLICT (id) DO NOTHING
        "#,
        completed_customer_record_id,
        tenant_id,
        branch_id,
        completed_assignment_id,
        karaoke_a_id,
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
            id, tenant_id, branch_id, actor_account_id, claimed_customer_id, idempotency_key
        ) VALUES ($1, $2, $3, $4, $5, $1)
        ON CONFLICT (id) DO NOTHING
        "#,
        urgent_batch_id,
        tenant_id,
        branch_id,
        staff_account_id,
        karaoke_a_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let urgent_report_insert: sqlx::postgres::PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_urgent_work_reports (
            id, tenant_id, branch_id, start_batch_id, employee_id, claimed_customer_id,
            status, created_by_account_id
        ) VALUES ($1, $2, $3, $4, $5, $6, 'completed', $7)
        ON CONFLICT (id) DO NOTHING
        "#,
        urgent_report_id,
        tenant_id,
        branch_id,
        urgent_batch_id,
        employee_id,
        karaoke_a_id,
        staff_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    let urgent_session_insert: sqlx::postgres::PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_urgent_work_sessions (
            id, tenant_id, branch_id, report_id, employee_id, started_at, ended_at,
            end_idempotency_key, started_by_account_id, start_source,
            ended_by_account_id, end_source
        ) VALUES (
            $1, $2, $3, $4, $5, CURRENT_TIMESTAMP - INTERVAL '5 hours',
            CURRENT_TIMESTAMP - INTERVAL '1 hour', $1, $6, 'self', $6, 'self'
        )
        ON CONFLICT (id) DO NOTHING
        "#,
        urgent_session_id,
        tenant_id,
        branch_id,
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
            id, tenant_id, branch_id, report_id, confirmed_customer_id,
            confirmed_started_at, confirmed_ended_at, customer_reference,
            notes, recorded_by_account_id
        )
        SELECT $1, $2, $3, $4, $5, session.started_at, session.ended_at,
               'DEV-MATCHED-001', 'Bằng chứng công việc phát sinh mẫu đã khớp', $6
        FROM business_urgent_work_sessions AS session
        WHERE session.tenant_id = $2 AND session.report_id = $3 AND session.ended_at IS NOT NULL
        ON CONFLICT (id) DO NOTHING
        "#,
        urgent_customer_record_id,
        tenant_id,
        branch_id,
        urgent_report_id,
        karaoke_a_id,
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

#[allow(clippy::too_many_arguments)]
async fn ensure_staffing_customer(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    branch_id: Uuid,
    code: &str,
    name: &str,
    address: &str,
    time_zone: &str,
    owner_account_id: Uuid,
) -> Result<Uuid, io::Error> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO business_customers (
            id, tenant_id, branch_id, code, name, address, time_zone, status,
            created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', $8, $8)
        ON CONFLICT (tenant_id, branch_id, lower(code)) DO UPDATE
        SET name = EXCLUDED.name,
            address = EXCLUDED.address,
            time_zone = EXCLUDED.time_zone,
            status = 'active',
            updated_at = CURRENT_TIMESTAMP,
            updated_by_account_id = EXCLUDED.updated_by_account_id
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        branch_id,
        code,
        name,
        address,
        time_zone,
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
    branch_id: Uuid,
    code: &str,
    name: &str,
    customer_id: Uuid,
    employee_id: Option<Uuid>,
    bill_hourly_rate: &str,
    worker_hourly_rate: &str,
    priority: i16,
    effective_from: NaiveDate,
    owner_account_id: Uuid,
) -> Result<(Uuid, Uuid), io::Error> {
    let customer_bill_rate_id: Uuid = ensure_staffing_hourly_rate(
        transaction,
        tenant_id,
        branch_id,
        "customer_bill",
        &format!("{code}-bill"),
        &format!("{name} - customer bill"),
        Some(customer_id),
        employee_id,
        bill_hourly_rate,
        priority,
        effective_from,
        owner_account_id,
    )
    .await?;
    let worker_pay_rate_id: Uuid = ensure_staffing_hourly_rate(
        transaction,
        tenant_id,
        branch_id,
        "worker_pay",
        &format!("{code}-pay"),
        &format!("{name} - worker pay"),
        Some(customer_id),
        employee_id,
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
    branch_id: Uuid,
    rate_kind: &str,
    code: &str,
    name: &str,
    customer_id: Option<Uuid>,
    employee_id: Option<Uuid>,
    hourly_rate: &str,
    priority: i16,
    effective_from: NaiveDate,
    owner_account_id: Uuid,
) -> Result<Uuid, io::Error> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO business_staffing_rates (
            id, tenant_id, branch_id, rate_kind, code, name, customer_id,
            employee_id, currency, hourly_rate, priority, effective_from,
            is_active, created_by_account_id
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, 'VND',
            $9::TEXT::NUMERIC, $10, $11, TRUE, $12
        )
        ON CONFLICT (tenant_id, branch_id, rate_kind, code, effective_from) DO UPDATE
        SET name = EXCLUDED.name,
            customer_id = EXCLUDED.customer_id,
            employee_id = EXCLUDED.employee_id,
            currency = EXCLUDED.currency,
            hourly_rate = EXCLUDED.hourly_rate,
            priority = EXCLUDED.priority,
            effective_to = NULL,
            is_active = TRUE
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        branch_id,
        rate_kind,
        code,
        name,
        customer_id,
        employee_id,
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
        "Seeding current Shepherd employee profiles: tenant_slug={} tenant_id={} employees={} effective_date={}",
        tenant.slug,
        tenant_id,
        accounts.len(),
        effective_date
    );

    let active_branches = sqlx::query!(
        "SELECT id FROM branches WHERE tenant_id = $1 AND status = 'active' ORDER BY code",
        tenant_id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    for branch in active_branches {
        ensure_dev_job(
            &mut transaction,
            tenant_id,
            branch.id,
            "employee",
            "Nhân viên phục vụ",
            owner_account_id,
        )
        .await?;
    }

    let mut employee_ids: HashMap<Uuid, Uuid> = HashMap::new();
    let mut employee_branch_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for account in accounts {
        if account.role == DevRole::TenantOwner {
            continue;
        }
        let employee_branch_id: Uuid = sqlx::query_scalar!(
            r#"
            SELECT assignment.branch_id
            FROM account_branch_assignments AS assignment
            INNER JOIN branches AS branch
                ON branch.tenant_id = assignment.tenant_id
               AND branch.id = assignment.branch_id
            WHERE assignment.tenant_id = $1 AND assignment.account_id = $2
            ORDER BY CASE WHEN branch.code = 'head-office' THEN 0 ELSE 1 END, branch.code
            LIMIT 1
            "#,
            tenant_id,
            account.id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(io::Error::other)?;
        let employee_code: String = account.username.to_ascii_lowercase();
        let employee_id: Uuid = sqlx::query_scalar!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, branch_id, account_id, employee_code, display_name, status, hire_date,
                created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $8, $8)
            ON CONFLICT (tenant_id, branch_id, lower(employee_code)) DO UPDATE
            SET account_id = EXCLUDED.account_id,
                display_name = EXCLUDED.display_name,
                status = 'active',
                termination_date = NULL,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = EXCLUDED.updated_by_account_id
            RETURNING id
            "#,
            Uuid::new_v4(),
            tenant_id,
            employee_branch_id,
            account.id,
            employee_code,
            account.username,
            effective_date,
            owner_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(io::Error::other)?;
        employee_ids.insert(account.id, employee_id);
        employee_branch_ids.insert(employee_id, employee_branch_id);
        debug!(
            "Development employee ensured: tenant_slug={} employee_id={} employee_code={} account_id={} branch_id={} role={}",
            tenant.slug,
            employee_id,
            employee_code,
            account.id,
            employee_branch_id,
            account.role.as_code()
        );
    }

    let (completed_attendance_sessions, open_attendance_sessions): (usize, usize) = seed_attendance_sessions(
        &mut transaction,
        tenant_id,
        tenant,
        accounts,
        &employee_ids,
        &employee_branch_ids,
    )
    .await?;

    transaction.commit().await.map_err(io::Error::other)?;
    info!(
        "Development employee profiles committed: tenant_slug={} tenant_id={} branches={} employees={} completed_attendance_sessions={} open_attendance_sessions={}",
        tenant.slug,
        tenant_id,
        DEV_BRANCHES.len(),
        employee_ids.len(),
        completed_attendance_sessions,
        open_attendance_sessions
    );
    Ok(())
}

async fn seed_attendance_sessions(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    tenant: &DevTenant,
    accounts: &[SeedAccount],
    employee_ids: &HashMap<Uuid, Uuid>,
    employee_branch_ids: &HashMap<Uuid, Uuid>,
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
        let branch_id: Uuid = employee_branch_ids
            .get(&employee_id)
            .copied()
            .ok_or_else(|| io::Error::other("seeded attendance branch mapping was not found"))?;

        ensure_completed_attendance_session(
            transaction,
            tenant_id,
            employee_id,
            branch_id,
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
            branch_id,
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
            branch_id,
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
            branch_id,
            account.id,
            dev_attendance_session_id(account.id, 5),
            3,
            night_start,
            night_end,
        )
        .await?;
        completed_count += 4;

        if account.username.to_ascii_lowercase().ends_with("staff_1") {
            let session_id: Uuid = dev_attendance_session_id(account.id, 4);
            let seeded: bool = sqlx::query_scalar!(
                r#"
                INSERT INTO hr_attendance_sessions (
                    id, tenant_id, branch_id, employee_id, check_in_at, check_in_by_account_id
                )
                SELECT
                    $1,
                    $2,
                    $5,
                    $3,
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
                    branch_id = EXCLUDED.branch_id,
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
                branch_id,
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
    branch_id: Uuid,
    account_id: Uuid,
    session_id: Uuid,
    days_ago: i32,
    check_in_time: NaiveTime,
    check_out_time: NaiveTime,
) -> Result<(), io::Error> {
    sqlx::query!(
        r#"
        INSERT INTO hr_attendance_sessions (
            id, tenant_id, branch_id, employee_id, check_in_at, check_out_at,
            check_in_by_account_id, check_out_by_account_id
        )
        VALUES (
            $1,
            $2,
            $8,
            $3,
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
            branch_id = EXCLUDED.branch_id,
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
        branch_id,
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
    branch_id: Uuid,
    account_id: Uuid,
    session_id: Uuid,
    start_days_ago: i32,
    check_in_time: NaiveTime,
    check_out_time: NaiveTime,
) -> Result<(), io::Error> {
    sqlx::query!(
        r#"
        INSERT INTO hr_attendance_sessions (
            id, tenant_id, branch_id, employee_id, check_in_at, check_out_at,
            check_in_by_account_id, check_out_by_account_id
        )
        VALUES (
            $1,
            $2,
            $8,
            $3,
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
            branch_id = EXCLUDED.branch_id,
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
        branch_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(io::Error::other)?;
    Ok(())
}

async fn ensure_dev_job(
    transaction: &mut infra_postgres::TenantTransaction,
    tenant_id: Uuid,
    branch_id: Uuid,
    code: &str,
    name: &str,
    owner_account_id: Uuid,
) -> Result<Uuid, io::Error> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO business_staffing_jobs (
            id, tenant_id, branch_id, code, name, status,
            created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, $3, $4, $5, 'active', $6, $6)
        ON CONFLICT (tenant_id, branch_id, lower(code)) DO UPDATE
        SET name = EXCLUDED.name,
            status = 'active',
            updated_at = CURRENT_TIMESTAMP,
            updated_by_account_id = EXCLUDED.updated_by_account_id
        RETURNING id
        "#,
        Uuid::new_v4(),
        tenant_id,
        branch_id,
        code,
        name,
        owner_account_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(io::Error::other)
}

async fn seed_branches(
    db: &DatabaseAdapter,
    tenant_id: Uuid,
    tenant: &DevTenant,
    accounts: &[SeedAccount],
    owner_account_id: Uuid,
) -> Result<(), io::Error> {
    let mut transaction = db.begin_tenant(tenant_id).await.map_err(io::Error::other)?;
    info!(
        "Seeding development branch hierarchy: tenant_slug={} tenant_id={} branches={}",
        tenant.slug,
        tenant_id,
        DEV_BRANCHES.len()
    );

    let mut branch_ids: Vec<Uuid> = Vec::with_capacity(DEV_BRANCHES.len());
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
            "Development branch ensured: tenant_slug={} branch_id={} branch_code={}",
            tenant.slug, branch_id, branch.code
        );
        branch_ids.push(branch_id);
    }

    for account in accounts {
        sqlx::query!(
            "DELETE FROM account_branch_assignments WHERE tenant_id = $1 AND account_id = $2",
            tenant_id,
            account.id,
        )
        .execute(transaction.connection())
        .await
        .map_err(io::Error::other)?;
        let assigned_branch_ids: Vec<Uuid> = match account.role {
            DevRole::TenantOwner => Vec::new(),
            DevRole::ExecutiveManager => branch_ids.clone(),
            DevRole::BranchManager | DevRole::Supervisor | DevRole::Staff => {
                let branch_code: &str = account.branch_code.as_deref().ok_or_else(|| {
                    io::Error::other(format!(
                        "branch-scoped development account '{}' has no branch code",
                        account.username
                    ))
                })?;
                let branch_index: usize = DEV_BRANCHES
                    .iter()
                    .position(|branch: &DevBranch| branch.code == branch_code)
                    .ok_or_else(|| io::Error::other(format!("unknown development branch code '{branch_code}'")))?;
                let branch_id: Uuid = branch_ids
                    .get(branch_index)
                    .copied()
                    .ok_or_else(|| io::Error::other("development branch ID mapping was not found"))?;
                vec![branch_id]
            }
        };
        for branch_id in assigned_branch_ids {
            sqlx::query!(
                r#"
                INSERT INTO account_branch_assignments (
                    tenant_id, account_id, branch_id, assigned_by_account_id
                ) VALUES ($1, $2, $3, $4)
                "#,
                tenant_id,
                account.id,
                branch_id,
                owner_account_id,
            )
            .execute(transaction.connection())
            .await
            .map_err(io::Error::other)?;
            debug!(
                tenant_slug = tenant.slug,
                tenant_id = %tenant_id,
                account_id = %account.id,
                branch_id = %branch_id,
                role = account.role.as_code(),
                "Development account branch assignment ensured"
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
    branch_code: Option<&str>,
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
        branch_code: branch_code.map(str::to_owned),
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
