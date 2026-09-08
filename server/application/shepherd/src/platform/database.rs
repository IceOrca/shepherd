use std::{error::Error, io};
use infra_auth::ext_service::auth_admin::{
    CreateExternalIdentityRequest, ExternalIdentity, ExtAuthAdmin, ExternalIdentityStatus,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use tracing::{error, warn, info, debug, trace};
use uuid::Uuid;
use super::core::{TenantBootstrapRequest, TenantBootstrapOwner};
use super::core::PlatformProfile;

pub(super) async fn administrator(
    pool: &PgPool,
    issuer: &str,
    subject: &str,
) -> Result<Option<PlatformProfile>, sqlx::Error> {
    sqlx::query_as!(
        PlatformProfile,
        "SELECT username, email FROM platform_administrators WHERE issuer = $1 AND subject = $2 AND is_active",
        issuer,
        subject
    )
    .fetch_optional(pool)
    .await
}

pub(super) async fn lock_administrator(
    pool: &PgPool,
    issuer: &str,
    subject: &str,
) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut transaction: Transaction<'static, Postgres> = pool.begin().await?;
    let active: Option<String> = sqlx::query_scalar!(
        "SELECT subject FROM platform_administrators WHERE issuer = $1 AND subject = $2 AND is_active FOR SHARE",
        issuer,
        subject
    )
    .fetch_optional(&mut *transaction)
    .await?;
    if active.is_none() {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(transaction)
}

pub(super) async fn audit_log_request(
    pool: &PgPool,
    issuer: &str,
    subject: &str,
    before: &str,
    level: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("INSERT INTO platform_administration_events (actor_issuer, actor_subject, action, details) VALUES ($1, $2, 'logging.level.requested', $3)", issuer, subject, json!({"before": before, "requested_level": level}))
        .execute(pool).await?;
    Ok(())
}
#[derive(Debug)]
pub(super) struct ResolvedOwner {
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

pub(super) async fn claim_bootstrap(
    pool: &PgPool,
    args: &TenantBootstrapRequest,
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

pub(super) async fn resolve_owner_identities(
    pool: &PgPool,
    auth_admin: &dyn ExtAuthAdmin,
    args: &TenantBootstrapRequest,
    owners: &[TenantBootstrapOwner],
    auth_issuer: &str,
) -> Result<Vec<ResolvedOwner>, Box<dyn Error + Send + Sync>> {
    let mut resolved: Vec<ResolvedOwner> = Vec::with_capacity(owners.len());
    for owner in owners {
        let identity: ExternalIdentity = if let Some(recovered) = auth_admin
            .find_provisioned_identity(&owner.email, args.tenant_id, args.idempotency_key)
            .await?
        {
            recovered
        } else if let Some(existing) = auth_admin.find_identity_by_email(&owner.email).await? {
            existing
        } else {
            auth_admin
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

pub(super) async fn commit_tenant(
    pool: &PgPool,
    args: &TenantBootstrapRequest,
    owners: &[ResolvedOwner],
    auth_issuer: &str,
    operator_account: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut transaction: Transaction<'_, Postgres> = pool.begin().await?;
    // Serialize identity membership decisions for competing tenant bootstraps.
    let mut subjects: Vec<&str> = owners
        .iter()
        .map(|owner: &ResolvedOwner| owner.subject.as_str())
        .collect();
    subjects.sort_unstable();
    for subject in subjects {
        sqlx::query!(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 702))",
            format!("{auth_issuer}:{subject}")
        )
        .execute(&mut *transaction)
        .await?;
        let already_mapped: bool = sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM account_identities WHERE issuer = $1 AND subject = $2 AND tenant_id <> $3) AS "exists!""#, auth_issuer, subject, args.tenant_id)
            .fetch_one(&mut *transaction).await?;
        if already_mapped {
            return Err(io::Error::other("owner identity is already mapped to another tenant").into());
        }
    }
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
        "platform_operator_issuer": auth_issuer,
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

pub(super) async fn mark_failed(pool: &PgPool, idempotency_key: Uuid, error_code: &str) {
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
