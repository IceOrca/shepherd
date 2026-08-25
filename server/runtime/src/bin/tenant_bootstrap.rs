#![cfg_attr(debug_assertions, allow(unused))]

use std::{error::Error, fs, io, path::Path};

use infra_auth::ext_service::auth_admin::{
    CreateExternalIdentityRequest, ExternalIdentity, ExternalIdentityAdmin, ExternalIdentityStatus,
};
use infra_kernel::debug::Debugging;
use infra_postgres::DatabaseAdapter;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use supabase_auth::SupabaseAuthIdentityAdmin;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug)]
struct BootstrapArgs {
    tenant_id: Uuid,
    tenant_slug: String,
    tenant_display_name: String,
    idempotency_key: Uuid,
    owners_file: String,
}

#[derive(Clone, Debug)]
struct OwnerInput {
    username: String,
    email: String,
    password: String,
}

#[derive(Debug)]
struct ResolvedOwner {
    account_id: Uuid,
    username: String,
    email: String,
    subject: String,
}

#[derive(Debug)]
struct BootstrapClaimRow {
    request_fingerprint: String,
    tenant_id: Uuid,
    tenant_slug: String,
    status: String,
}

#[derive(Debug)]
struct IdentityTenantRow {
    tenant_id: Uuid,
}

#[derive(Debug)]
struct ExistingTenantRow {
    slug: String,
    display_name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    load_environment()?;
    Debugging::init();
    let args: BootstrapArgs = parse_args()?;
    let owners: Vec<OwnerInput> = load_owners(Path::new(&args.owners_file))?;
    let operator_account: String = authenticate_operator()?;
    let operator_email: String = normalized_required_env("TENANT_BOOTSTRAP_ADMIN_EMAIL")?;
    let auth_issuer: String = required_env("AUTH_ISSUER_URL")?;
    let request_fingerprint: String = fingerprint_request(&args, &owners);

    info!(
        tenant_id = %args.tenant_id,
        tenant_slug = args.tenant_slug,
        owner_count = owners.len(),
        idempotency_key = %args.idempotency_key,
        operator_account,
        "Platform tenant bootstrap started"
    );

    let db: std::sync::Arc<DatabaseAdapter> = DatabaseAdapter::new_arc().await;
    let completed: bool = claim_bootstrap(
        db.global_pool(),
        &args,
        &request_fingerprint,
        &operator_account,
        &operator_email,
        owners.len(),
    )
    .await?;
    if completed {
        info!(
            tenant_id = %args.tenant_id,
            tenant_slug = args.tenant_slug,
            idempotency_key = %args.idempotency_key,
            "Platform tenant bootstrap replay returned the completed result"
        );
        print_result(&args, owners.len(), true);
        return Ok(());
    }

    let identity_admin: std::sync::Arc<SupabaseAuthIdentityAdmin> = SupabaseAuthIdentityAdmin::from_env()?;
    let resolved_owners: Vec<ResolvedOwner> =
        match resolve_owner_identities(db.global_pool(), identity_admin.as_ref(), &args, &owners, &auth_issuer).await {
            Ok(resolved) => resolved,
            Err(resolve_error) => {
                mark_failed(db.global_pool(), args.idempotency_key, "external_identity_resolution").await;
                return Err(resolve_error);
            }
        };

    if let Err(database_error) = commit_tenant(
        db.global_pool(),
        &args,
        &resolved_owners,
        &auth_issuer,
        &operator_account,
    )
    .await
    {
        mark_failed(db.global_pool(), args.idempotency_key, "tenant_transaction").await;
        return Err(database_error);
    }

    info!(
        tenant_id = %args.tenant_id,
        tenant_slug = args.tenant_slug,
        owner_count = resolved_owners.len(),
        idempotency_key = %args.idempotency_key,
        "Platform tenant bootstrap completed"
    );
    print_result(&args, resolved_owners.len(), false);
    Ok(())
}

fn load_environment() -> Result<(), io::Error> {
    if std::env::var("APP_ENV").as_deref() == Ok("production") {
        dotenvy::from_path(Path::new("/run/secrets/server_prod_env"))
            .map_err(|error: dotenvy::Error| io::Error::other(format!("load production environment: {error}")))?;
    } else {
        let _loaded: Result<std::path::PathBuf, dotenvy::Error> = dotenvy::dotenv();
    }
    Ok(())
}

fn parse_args() -> Result<BootstrapArgs, io::Error> {
    let mut tenant_id: Option<Uuid> = None;
    let mut tenant_slug: Option<String> = None;
    let mut tenant_display_name: Option<String> = None;
    let mut idempotency_key: Option<Uuid> = None;
    let mut owners_file: Option<String> = None;
    let mut arguments: std::env::Args = std::env::args();
    let _binary_name: Option<String> = arguments.next();
    while let Some(flag) = arguments.next() {
        let value: String = arguments
            .next()
            .ok_or_else(|| io::Error::other(format!("missing value after '{flag}'")))?;
        match flag.as_str() {
            "--tenant-id" => tenant_id = Some(parse_uuid("tenant ID", &value)?),
            "--slug" => tenant_slug = Some(value.trim().to_owned()),
            "--name" => tenant_display_name = Some(value.trim().to_owned()),
            "--idempotency-key" => idempotency_key = Some(parse_uuid("idempotency key", &value)?),
            "--owners-file" => owners_file = Some(value),
            unsupported => return Err(io::Error::other(format!("unsupported argument '{unsupported}'"))),
        }
    }
    let args: BootstrapArgs = BootstrapArgs {
        tenant_id: tenant_id.ok_or_else(|| io::Error::other("--tenant-id is required"))?,
        tenant_slug: tenant_slug.ok_or_else(|| io::Error::other("--slug is required"))?,
        tenant_display_name: tenant_display_name.ok_or_else(|| io::Error::other("--name is required"))?,
        idempotency_key: idempotency_key.ok_or_else(|| io::Error::other("--idempotency-key is required"))?,
        owners_file: owners_file.ok_or_else(|| io::Error::other("--owners-file is required"))?,
    };
    if args.tenant_slug.len() < 2
        || args.tenant_slug.len() > 63
        || !args
            .tenant_slug
            .bytes()
            .all(|byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(io::Error::other(
            "tenant slug must contain 2-63 lowercase ASCII letters, digits, or hyphens",
        ));
    }
    if args.tenant_display_name.is_empty() || args.tenant_display_name.len() > 200 {
        return Err(io::Error::other("tenant name must contain 1-200 characters"));
    }
    Ok(args)
}

fn load_owners(path: &Path) -> Result<Vec<OwnerInput>, io::Error> {
    let contents: String = fs::read_to_string(path)
        .map_err(|error: io::Error| io::Error::other(format!("read {}: {error}", path.display())))?;
    let mut owners: Vec<OwnerInput> = Vec::new();
    for (line_index, raw_line) in contents.lines().enumerate() {
        let line_number: usize = line_index + 1;
        if raw_line.trim().is_empty() || raw_line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = raw_line.split('\t').collect();
        let [username_raw, email_raw, password_raw] = columns.as_slice() else {
            return Err(io::Error::other(format!(
                "{}:{line_number} must contain username, email, and password separated by tabs",
                path.display()
            )));
        };
        let username: String = username_raw.trim().to_owned();
        let email: String = email_raw.trim().to_lowercase();
        let password: String = (*password_raw).to_owned();
        if username.len() < 3 || username.len() > 128 {
            return Err(io::Error::other(format!(
                "{}:{line_number} has an invalid username",
                path.display()
            )));
        }
        if !valid_email(&email) {
            return Err(io::Error::other(format!(
                "{}:{line_number} has an invalid email",
                path.display()
            )));
        }
        if password.len() < 8 {
            return Err(io::Error::other(format!(
                "{}:{line_number} password must contain at least 8 characters",
                path.display()
            )));
        }
        if owners.iter().any(|owner: &OwnerInput| {
            owner.username.eq_ignore_ascii_case(&username) || owner.email.eq_ignore_ascii_case(&email)
        }) {
            return Err(io::Error::other(format!(
                "{}:{line_number} duplicates an owner username or email",
                path.display()
            )));
        }
        owners.push(OwnerInput {
            username,
            email,
            password,
        });
    }
    if owners.is_empty() {
        return Err(io::Error::other("at least one tenant owner is required"));
    }
    Ok(owners)
}

fn authenticate_operator() -> Result<String, io::Error> {
    let expected_account: String = required_env("TENANT_BOOTSTRAP_ADMIN_ACCOUNT")?;
    let presented_account: String = required_env("TENANT_BOOTSTRAP_PRESENTED_ACCOUNT")?;
    let expected_secret: String = match std::env::var("TENANT_BOOTSTRAP_ADMIN_SECRET_FILE") {
        Ok(path) if !path.trim().is_empty() => fs::read_to_string(path.trim())
            .map(|secret: String| secret.trim_end_matches(['\r', '\n']).to_owned())
            .map_err(|error: io::Error| io::Error::other(format!("read bootstrap administrator secret: {error}")))?,
        _ => required_env("TENANT_BOOTSTRAP_ADMIN_SECRET")?,
    };
    let presented_secret: String = required_env("TENANT_BOOTSTRAP_PRESENTED_SECRET")?;
    if !constant_time_equal(expected_account.as_bytes(), presented_account.as_bytes())
        || !constant_time_equal(expected_secret.as_bytes(), presented_secret.as_bytes())
    {
        warn!(
            presented_account,
            "Platform tenant bootstrap administrator authentication rejected"
        );
        return Err(io::Error::other("bootstrap administrator authentication failed"));
    }
    info!(
        operator_account = expected_account,
        "Platform tenant bootstrap administrator authenticated"
    );
    Ok(expected_account)
}

async fn claim_bootstrap(
    pool: &PgPool,
    args: &BootstrapArgs,
    fingerprint: &str,
    operator_account: &str,
    operator_email: &str,
    owner_count: usize,
) -> Result<bool, sqlx::Error> {
    let owner_count: i32 = i32::try_from(owner_count).map_err(|conversion_error: std::num::TryFromIntError| {
        sqlx::Error::Protocol(format!("owner count is too large: {conversion_error}"))
    })?;
    let inserted: sqlx::postgres::PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO platform_tenant_bootstrap_requests (
            idempotency_key, request_fingerprint, tenant_id, tenant_slug,
            tenant_display_name, operator_account, operator_email, owner_count
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (idempotency_key) DO NOTHING
        "#,
        args.idempotency_key,
        fingerprint,
        args.tenant_id,
        args.tenant_slug,
        args.tenant_display_name,
        operator_account,
        operator_email,
        owner_count,
    )
    .execute(pool)
    .await?;
    debug!(
        idempotency_key = %args.idempotency_key,
        inserted = inserted.rows_affected() == 1,
        "Platform tenant bootstrap claim checked"
    );
    let claim: BootstrapClaimRow = sqlx::query_as!(
        BootstrapClaimRow,
        r#"
        SELECT request_fingerprint, tenant_id, tenant_slug, status
        FROM platform_tenant_bootstrap_requests
        WHERE idempotency_key = $1
        "#,
        args.idempotency_key,
    )
    .fetch_one(pool)
    .await?;
    if claim.request_fingerprint != fingerprint
        || claim.tenant_id != args.tenant_id
        || claim.tenant_slug != args.tenant_slug
    {
        return Err(sqlx::Error::Protocol(
            "idempotency key was already used for different tenant bootstrap input".to_owned(),
        ));
    }
    if claim.status == "completed" {
        return Ok(true);
    }
    sqlx::query!(
        r#"
        UPDATE platform_tenant_bootstrap_requests
        SET status = 'processing', last_error_code = NULL, completed_at = NULL,
            updated_at = CURRENT_TIMESTAMP
        WHERE idempotency_key = $1
        "#,
        args.idempotency_key,
    )
    .execute(pool)
    .await?;
    Ok(false)
}

async fn resolve_owner_identities(
    pool: &PgPool,
    identity_admin: &dyn ExternalIdentityAdmin,
    args: &BootstrapArgs,
    owners: &[OwnerInput],
    auth_issuer: &str,
) -> Result<Vec<ResolvedOwner>, Box<dyn Error>> {
    let mut resolved: Vec<ResolvedOwner> = Vec::with_capacity(owners.len());
    for owner in owners {
        let identity: ExternalIdentity = if let Some(recovered) = identity_admin
            .find_provisioned_identity(&owner.email, args.tenant_id, args.idempotency_key)
            .await?
        {
            recovered
        } else if let Some(existing) = identity_admin.find_identity_by_email(&owner.email).await? {
            existing
        } else {
            identity_admin
                .create_identity(&CreateExternalIdentityRequest {
                    username: owner.username.clone(),
                    email: owner.email.clone(),
                    password: Some(owner.password.clone()),
                    tenant_id: args.tenant_id,
                    idempotency_key: args.idempotency_key,
                })
                .await?
        };
        if identity.status != ExternalIdentityStatus::Active {
            return Err(io::Error::other(format!("owner identity '{}' is disabled", owner.email)).into());
        }
        if identity
            .email
            .as_deref()
            .is_none_or(|email: &str| !email.eq_ignore_ascii_case(&owner.email))
        {
            return Err(io::Error::other(format!("provider returned the wrong email for '{}'", owner.email)).into());
        }
        let memberships: Vec<IdentityTenantRow> = sqlx::query_as!(
            IdentityTenantRow,
            r#"
            SELECT tenant_id
            FROM account_identities
            WHERE issuer = $1 AND subject = $2
            "#,
            auth_issuer,
            identity.subject,
        )
        .fetch_all(pool)
        .await?;
        if memberships
            .iter()
            .any(|membership: &IdentityTenantRow| membership.tenant_id != args.tenant_id)
        {
            return Err(io::Error::other(format!(
                "owner email '{}' is already mapped to another tenant",
                owner.email
            ))
            .into());
        }
        info!(
            tenant_id = %args.tenant_id,
            owner_email = owner.email,
            auth_subject = identity.subject,
            "Tenant owner external identity resolved"
        );
        resolved.push(ResolvedOwner {
            account_id: Uuid::new_v4(),
            username: owner.username.clone(),
            email: owner.email.clone(),
            subject: identity.subject,
        });
    }
    let subject_values: Value = Value::Array(
        resolved
            .iter()
            .map(|owner: &ResolvedOwner| json!({ "email": owner.email, "subject": owner.subject }))
            .collect(),
    );
    sqlx::query!(
        r#"
        UPDATE platform_tenant_bootstrap_requests
        SET auth_subjects = $2, updated_at = CURRENT_TIMESTAMP
        WHERE idempotency_key = $1 AND status = 'processing'
        "#,
        args.idempotency_key,
        subject_values,
    )
    .execute(pool)
    .await?;
    Ok(resolved)
}

async fn commit_tenant(
    pool: &PgPool,
    args: &BootstrapArgs,
    owners: &[ResolvedOwner],
    auth_issuer: &str,
    operator_account: &str,
) -> Result<(), Box<dyn Error>> {
    let mut transaction: Transaction<'_, Postgres> = pool.begin().await?;
    let existing_tenant: Option<ExistingTenantRow> = sqlx::query_as!(
        ExistingTenantRow,
        "SELECT slug, display_name FROM tenants WHERE id = $1 OR lower(slug) = lower($2)",
        args.tenant_id,
        args.tenant_slug,
    )
    .fetch_optional(&mut *transaction)
    .await?;
    if let Some(existing) = existing_tenant {
        return Err(io::Error::other(format!(
            "tenant already exists as '{}' / '{}'; use the original completed idempotency key instead",
            existing.slug, existing.display_name
        ))
        .into());
    }
    sqlx::query!(
        r#"
        INSERT INTO tenants (id, slug, display_name, status)
        VALUES ($1, $2, $3, 'active')
        "#,
        args.tenant_id,
        args.tenant_slug,
        args.tenant_display_name,
    )
    .execute(&mut *transaction)
    .await?;
    let _tenant_context = sqlx::query!(
        "SELECT set_config('app.tenant_id', $1, TRUE)",
        args.tenant_id.to_string()
    )
    .fetch_one(&mut *transaction)
    .await?;
    let first_owner_id: Uuid = owners
        .first()
        .map(|owner: &ResolvedOwner| owner.account_id)
        .ok_or_else(|| io::Error::other("at least one resolved owner is required"))?;
    for owner in owners {
        sqlx::query!(
            r#"
            INSERT INTO accounts (
                id, tenant_id, username, email, status, primary_role_code,
                created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, 'active', 'tenant_owner', NULL, NULL)
            "#,
            owner.account_id,
            args.tenant_id,
            owner.username,
            owner.email,
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO account_roles (tenant_id, account_id, role_code, assigned_by_account_id)
            VALUES ($1, $2, 'tenant_owner', $3)
            "#,
            args.tenant_id,
            owner.account_id,
            first_owner_id,
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO account_identities (issuer, subject, tenant_id, account_id)
            VALUES ($1, $2, $3, $4)
            "#,
            auth_issuer,
            owner.subject,
            args.tenant_id,
            owner.account_id,
        )
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query!(
        r#"
        UPDATE accounts
        SET created_by_account_id = $2, updated_by_account_id = $2,
            updated_at = CURRENT_TIMESTAMP
        WHERE tenant_id = $1
          AND id = ANY($3::UUID[])
        "#,
        args.tenant_id,
        first_owner_id,
        &owners
            .iter()
            .map(|owner: &ResolvedOwner| owner.account_id)
            .collect::<Vec<Uuid>>(),
    )
    .execute(&mut *transaction)
    .await?;
    let audit_after: Value = json!({
        "tenant_slug": args.tenant_slug,
        "tenant_display_name": args.tenant_display_name,
        "owner_account_ids": owners.iter().map(|owner: &ResolvedOwner| owner.account_id).collect::<Vec<Uuid>>(),
        "platform_operator": operator_account,
        "idempotency_key": args.idempotency_key,
    });
    sqlx::query!(
        r#"
        INSERT INTO access_control_audit_log (
            tenant_id, actor_account_id, action, object_type, object_id, after_value
        )
        VALUES ($1, $2, 'tenant.bootstrap', 'tenant', $3, $4)
        "#,
        args.tenant_id,
        first_owner_id,
        args.tenant_id.to_string(),
        audit_after,
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query!(
        r#"
        UPDATE platform_tenant_bootstrap_requests
        SET status = 'completed', last_error_code = NULL,
            completed_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
        WHERE idempotency_key = $1 AND status = 'processing'
        "#,
        args.idempotency_key,
    )
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn mark_failed(pool: &PgPool, idempotency_key: Uuid, error_code: &str) {
    let result: Result<sqlx::postgres::PgQueryResult, sqlx::Error> = sqlx::query!(
        r#"
        UPDATE platform_tenant_bootstrap_requests
        SET status = 'failed', last_error_code = $2, completed_at = NULL,
            updated_at = CURRENT_TIMESTAMP
        WHERE idempotency_key = $1 AND status <> 'completed'
        "#,
        idempotency_key,
        error_code,
    )
    .execute(pool)
    .await;
    match result {
        Ok(update) => warn!(
            idempotency_key = %idempotency_key,
            error_code,
            rows_affected = update.rows_affected(),
            "Platform tenant bootstrap marked failed; provider identities were retained for retry"
        ),
        Err(update_error) => error!(
            idempotency_key = %idempotency_key,
            error_code,
            error = %update_error,
            "Platform tenant bootstrap failure status could not be persisted"
        ),
    }
}

fn fingerprint_request(args: &BootstrapArgs, owners: &[OwnerInput]) -> String {
    let mut digest: Sha256 = Sha256::new();
    update_fingerprint(&mut digest, &args.tenant_id.to_string());
    update_fingerprint(&mut digest, &args.tenant_slug);
    update_fingerprint(&mut digest, &args.tenant_display_name);
    for owner in owners {
        update_fingerprint(&mut digest, &owner.username);
        update_fingerprint(&mut digest, &owner.email);
        update_fingerprint(&mut digest, &owner.password);
    }
    format!("{:x}", digest.finalize())
}

fn update_fingerprint(digest: &mut Sha256, value: &str) {
    digest.update(value.len().to_be_bytes());
    digest.update(value.as_bytes());
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let maximum_length: usize = left.len().max(right.len());
    let mut difference: usize = left.len() ^ right.len();
    for offset in 0..maximum_length {
        let left_byte: u8 = left.get(offset).copied().unwrap_or(0);
        let right_byte: u8 = right.get(offset).copied().unwrap_or(0);
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

fn parse_uuid(label: &str, value: &str) -> Result<Uuid, io::Error> {
    Uuid::parse_str(value).map_err(|error: uuid::Error| io::Error::other(format!("invalid {label}: {error}")))
}

fn required_env(name: &str) -> Result<String, io::Error> {
    std::env::var(name)
        .ok()
        .map(|value: String| value.trim().to_owned())
        .filter(|value: &String| !value.is_empty())
        .ok_or_else(|| io::Error::other(format!("{name} is required")))
}

fn normalized_required_env(name: &str) -> Result<String, io::Error> {
    let value: String = required_env(name)?.to_lowercase();
    if !valid_email(&value) {
        return Err(io::Error::other(format!("{name} must be a valid email")));
    }
    Ok(value)
}

fn valid_email(value: &str) -> bool {
    let mut parts: std::str::Split<'_, char> = value.split('@');
    let local: Option<&str> = parts.next();
    let domain: Option<&str> = parts.next();
    local.is_some_and(|part: &str| !part.is_empty())
        && domain.is_some_and(|part: &str| part.contains('.') && !part.starts_with('.') && !part.ends_with('.'))
        && parts.next().is_none()
        && !value.chars().any(char::is_whitespace)
}

fn print_result(args: &BootstrapArgs, owner_count: usize, replayed: bool) {
    println!("tenant_bootstrap_status=completed");
    println!("tenant_id={}", args.tenant_id);
    println!("tenant_slug={}", args.tenant_slug);
    println!("owner_count={owner_count}");
    println!("idempotency_key={}", args.idempotency_key);
    println!("idempotent_replay={replayed}");
    println!("provider_identities_retained_on_failure=true");
}
