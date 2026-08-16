use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use infra_kernel::debug::*;
use crate::{
    AccountMutationError, AccountRepo, StoreAccountError,
    account::{
        AccountPermission, AccountStatus, AccountSummary, AuthorizationCatalog, PermissionSummary, Role, RoleSummary,
        UserAccount,
    },
};
use sqlx::postgres::PgQueryResult;
use uuid::Uuid;

use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};

pub struct AuthProvider {
    database: Arc<DatabaseAdapter>,
}

impl AuthProvider {
    pub fn new_arc(database: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { database })
    }

    async fn begin_active_tenant(&self, tenant_id: Uuid) -> Result<Option<TenantTransaction>, String> {
        match self.database.begin_tenant(tenant_id).await {
            Ok(transaction) => Ok(Some(transaction)),
            Err(TenantDbErr::TenantInactive(_)) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    async fn mutation_transaction(&self, tenant_id: Uuid) -> Result<TenantTransaction, AccountMutationError> {
        self.begin_active_tenant(tenant_id)
            .await
            .map_err(|error: String| {
                log_error!(
                    "Failed to open tenant account transaction: tenant_id={} error={}",
                    tenant_id,
                    error
                );
                AccountMutationError::BackendUnavailable
            })?
            .ok_or(AccountMutationError::BackendUnavailable)
    }
}

#[async_trait]
impl AccountRepo for AuthProvider {
    async fn resolve_active_tenant_id(&self, tenant: &str) -> Result<Option<Uuid>, String> {
        log_debug!("Resolving active tenant login slug: tenant={}", tenant);
        let tenant_id: Option<Uuid> = self
            .database
            .resolve_active_tenant_id(tenant)
            .await
            .map_err(|error: TenantDbErr| error.to_string())?;

        match tenant_id {
            Some(tenant_id) => log_debug!(
                "Active tenant login slug resolved: tenant={} tenant_id={}",
                tenant,
                tenant_id
            ),
            None => log_info!("No active tenant found for login slug: tenant={}", tenant),
        }
        Ok(tenant_id)
    }

    async fn find_by_username(&self, tenant_id: Uuid, username: &str) -> Result<Option<UserAccount>, String> {
        let Some(mut transaction) = self.begin_active_tenant(tenant_id).await? else {
            return Ok(None);
        };

        let account = sqlx::query!(
            r#"
            SELECT account.id, account.username, account.password_hash, account.status,
                   account.auth_version, account.primary_role_code
            FROM accounts AS account
            JOIN roles AS primary_role
              ON primary_role.code = account.primary_role_code
             AND primary_role.is_active
            JOIN account_roles AS primary_assignment
              ON primary_assignment.tenant_id = account.tenant_id
             AND primary_assignment.account_id = account.id
             AND primary_assignment.role_code = account.primary_role_code
            WHERE account.tenant_id = $1
              AND lower(account.username) = lower($2)
            "#,
            tenant_id,
            username.trim(),
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| error.to_string())?;
        let Some(account) = account else {
            return Ok(None);
        };
        let account_id: Uuid = account.id;
        let role: Role = Role::from_code(&account.primary_role_code)
            .ok_or_else(|| format!("unsupported primary role '{}'", account.primary_role_code))?;

        let roles: Vec<String> = sqlx::query_scalar!(
            r#"
            SELECT account_role.role_code AS "role_code!"
            FROM account_roles AS account_role
            JOIN roles AS role ON role.code = account_role.role_code AND role.is_active
            WHERE account_role.tenant_id = $1
              AND account_role.account_id = $2
            ORDER BY account_role.role_code
            "#,
            tenant_id,
            account_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| error.to_string())?;

        let permissions: Vec<String> = sqlx::query_scalar!(
            r#"
            SELECT permission_code AS "permission_code!"
            FROM (
                SELECT role_permission.permission_code
                FROM account_roles AS account_role
                JOIN roles AS role ON role.code = account_role.role_code AND role.is_active
                JOIN role_permissions AS role_permission ON role_permission.role_code = account_role.role_code
                WHERE account_role.tenant_id = $1
                  AND account_role.account_id = $2
                UNION
                SELECT account_permission.permission_code
                FROM account_permissions AS account_permission
                WHERE account_permission.tenant_id = $1
                  AND account_permission.account_id = $2
                  AND account_permission.effect = 'allow'
                  AND (account_permission.expires_at IS NULL OR account_permission.expires_at > CURRENT_TIMESTAMP)
                EXCEPT
                SELECT account_permission.permission_code
                FROM account_permissions AS account_permission
                WHERE account_permission.tenant_id = $1
                  AND account_permission.account_id = $2
                  AND account_permission.effect = 'deny'
                  AND (account_permission.expires_at IS NULL OR account_permission.expires_at > CURRENT_TIMESTAMP)
            ) effective_permissions
            ORDER BY permission_code
            "#,
            tenant_id,
            account_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| error.to_string())?;

        log_debug!(
            "Loaded account authorization state: tenant_id={} account_id={} primary_role={} active_roles={} permissions={} auth_version={}",
            tenant_id,
            account_id,
            role.as_code(),
            roles.len(),
            permissions.len(),
            account.auth_version
        );
        Ok(Some(UserAccount {
            id: account_id,
            username: account.username,
            passphrase_key: account.password_hash,
            role,
            roles,
            active: account.status == "active",
            auth_version: account.auth_version,
            permissions,
        }))
    }

    async fn store(
        &self,
        tenant_id: Uuid,
        account: &UserAccount,
        audit_account_id: Option<Uuid>,
    ) -> Result<(), StoreAccountError> {
        let mut transaction: TenantTransaction = self
            .begin_active_tenant(tenant_id)
            .await
            .map_err(|_| StoreAccountError::BackendUnavailable)?
            .ok_or(StoreAccountError::BackendUnavailable)?;

        let insert_account: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
            r#"
            INSERT INTO accounts (
                id, tenant_id, username, password_hash, status, primary_role_code,
                created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, 'active', $5, $6, $6)
            "#,
            account.id,
            tenant_id,
            account.username.trim(),
            account.passphrase_key,
            account.role.as_code(),
            audit_account_id,
        )
        .execute(transaction.connection())
        .await;
        if let Err(error) = insert_account {
            log_error!(
                "Account insert failed: tenant_id={} username={} error={}",
                tenant_id,
                account.username,
                error
            );
            return Err(
                if error
                    .as_database_error()
                    .is_some_and(|database_error| database_error.is_unique_violation())
                {
                    StoreAccountError::UsernameAlreadyExists
                } else {
                    StoreAccountError::BackendUnavailable
                },
            );
        }

        sqlx::query!(
            r#"
            INSERT INTO account_roles (tenant_id, account_id, role_code, assigned_by_account_id)
            VALUES ($1, $2, $3, $4)
            "#,
            tenant_id,
            account.id,
            account.role.as_code(),
            audit_account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error| {
            log_error!(
                "Primary account role insert failed: tenant_id={} account_id={} role={} error={}",
                tenant_id,
                account.id,
                account.role.as_code(),
                error
            );
            StoreAccountError::BackendUnavailable
        })?;
        transaction.commit().await.map_err(|error| {
            log_error!(
                "Account creation commit failed: tenant_id={} account_id={} error={}",
                tenant_id,
                account.id,
                error
            );
            StoreAccountError::BackendUnavailable
        })
    }

    async fn list_accounts(&self, tenant_id: Uuid) -> Result<Vec<AccountSummary>, String> {
        let Some(mut transaction) = self.begin_active_tenant(tenant_id).await? else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query!(
            r#"
            SELECT id, username, status, primary_role_code, auth_version,
                   password_changed_at, last_authenticated_at, created_at, updated_at
            FROM accounts
            WHERE tenant_id = $1
            ORDER BY lower(username), id
            "#,
            tenant_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| error.to_string())?;
        let mut accounts: Vec<AccountSummary> = Vec::with_capacity(rows.len());
        for row in rows {
            let roles: Vec<String> = sqlx::query_scalar!(
                r#"
                SELECT account_role.role_code AS "role_code!"
                FROM account_roles AS account_role
                JOIN roles AS role ON role.code = account_role.role_code AND role.is_active
                WHERE account_role.tenant_id = $1
                  AND account_role.account_id = $2
                ORDER BY account_role.role_code
                "#,
                tenant_id,
                row.id,
            )
            .fetch_all(transaction.connection())
            .await
            .map_err(|error| error.to_string())?;
            accounts.push(AccountSummary {
                id: row.id,
                username: row.username,
                status: AccountStatus::from_code(&row.status)
                    .ok_or_else(|| format!("unsupported account status '{}'", row.status))?,
                primary_role: Role::from_code(&row.primary_role_code)
                    .ok_or_else(|| format!("unsupported primary role '{}'", row.primary_role_code))?,
                roles,
                auth_version: row.auth_version,
                password_changed_at: row.password_changed_at,
                last_authenticated_at: row.last_authenticated_at,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }
        log_debug!(
            "Listed tenant accounts: tenant_id={} account_count={}",
            tenant_id,
            accounts.len()
        );
        Ok(accounts)
    }

    async fn list_authorization_catalog(&self, tenant_id: Uuid) -> Result<AuthorizationCatalog, String> {
        let Some(mut transaction) = self.begin_active_tenant(tenant_id).await? else {
            return Err("tenant is not active".to_owned());
        };
        let role_rows = sqlx::query!(
            r#"
            SELECT code, display_name, description, is_system, is_active
            FROM roles
            ORDER BY is_system DESC, code
            "#
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| error.to_string())?;
        let mut roles: Vec<RoleSummary> = Vec::with_capacity(role_rows.len());
        for role in role_rows {
            let permissions: Vec<String> = sqlx::query_scalar!(
                r#"
                SELECT permission_code AS "permission_code!"
                FROM role_permissions
                WHERE role_code = $1
                ORDER BY permission_code
                "#,
                role.code,
            )
            .fetch_all(transaction.connection())
            .await
            .map_err(|error| error.to_string())?;
            roles.push(RoleSummary {
                code: role.code,
                display_name: role.display_name,
                description: role.description,
                is_system: role.is_system,
                is_active: role.is_active,
                permissions,
            });
        }
        let permissions: Vec<PermissionSummary> = sqlx::query_as!(
            PermissionSummary,
            "SELECT code, description FROM permissions ORDER BY code"
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| error.to_string())?;
        log_debug!(
            "Listed tenant authorization catalog: tenant_id={} roles={} permissions={}",
            tenant_id,
            roles.len(),
            permissions.len()
        );
        Ok(AuthorizationCatalog { roles, permissions })
    }

    async fn mark_authenticated(&self, tenant_id: Uuid, account_id: Uuid) -> Result<(), String> {
        let Some(mut transaction) = self.begin_active_tenant(tenant_id).await? else {
            return Err("tenant is not active".to_owned());
        };
        let result: PgQueryResult = sqlx::query!(
            r#"
            UPDATE accounts
            SET last_authenticated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2 AND status = 'active'
            "#,
            tenant_id,
            account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error| error.to_string())?;
        if result.rows_affected() != 1 {
            return Err("account became unavailable during authentication".to_owned());
        }
        transaction.commit().await.map_err(|error| error.to_string())?;
        log_debug!(
            "Successful-login audit persisted: tenant_id={} account_id={}",
            tenant_id,
            account_id
        );
        Ok(())
    }

    async fn update_password(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        passphrase_key: &str,
        audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        let mut transaction: TenantTransaction = self.mutation_transaction(tenant_id).await?;
        let result: PgQueryResult = sqlx::query!(
            r#"
            UPDATE accounts
            SET password_hash = $3,
                password_changed_at = CURRENT_TIMESTAMP,
                auth_version = auth_version + 1,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $4
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            account_id,
            passphrase_key,
            audit_account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error| {
            log_error!(
                "Account password update failed: account_id={} error={}",
                account_id,
                error
            );
            AccountMutationError::BackendUnavailable
        })?;
        if result.rows_affected() != 1 {
            return Err(AccountMutationError::AccountNotFound);
        }
        transaction
            .commit()
            .await
            .map_err(|_| AccountMutationError::BackendUnavailable)?;
        log_notice!(
            "Account password changed and auth version advanced: tenant_id={} account_id={} actor_account_id={}",
            tenant_id,
            account_id,
            audit_account_id
        );
        Ok(())
    }

    async fn update_status(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        status: AccountStatus,
        audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        let mut transaction: TenantTransaction = self.mutation_transaction(tenant_id).await?;
        let current = sqlx::query!(
            "SELECT primary_role_code FROM accounts WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
            tenant_id,
            account_id
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?
        .ok_or(AccountMutationError::AccountNotFound)?;
        if current.primary_role_code == Role::TenantOwner.as_code() && status != AccountStatus::Active {
            ensure_another_active_owner(transaction.connection(), tenant_id, account_id).await?;
        }
        sqlx::query!(
            r#"
            UPDATE accounts
            SET status = $3,
                auth_version = auth_version + 1,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $4
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            account_id,
            status.as_code(),
            audit_account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| AccountMutationError::BackendUnavailable)?;
        log_notice!(
            "Account status changed: tenant_id={} account_id={} status={} actor_account_id={}",
            tenant_id,
            account_id,
            status.as_code(),
            audit_account_id
        );
        Ok(())
    }

    async fn replace_roles(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        primary_role: Role,
        roles: &[String],
        audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        let mut role_codes: BTreeSet<String> = roles.iter().map(|role: &String| role.trim().to_owned()).collect();
        role_codes.insert(primary_role.as_code().to_owned());
        if role_codes.is_empty() || role_codes.len() > 64 || role_codes.iter().any(|role| !role_code_is_valid(role)) {
            return Err(AccountMutationError::InvalidRole);
        }

        let mut transaction: TenantTransaction = self.mutation_transaction(tenant_id).await?;
        let current = sqlx::query!(
            "SELECT primary_role_code FROM accounts WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
            tenant_id,
            account_id
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?
        .ok_or(AccountMutationError::AccountNotFound)?;
        if current.primary_role_code == Role::TenantOwner.as_code() && primary_role != Role::TenantOwner {
            ensure_another_active_owner(transaction.connection(), tenant_id, account_id).await?;
        }
        let role_codes: Vec<String> = role_codes.into_iter().collect();
        let valid_role_count: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM roles WHERE code = ANY($1) AND is_active"#,
            &role_codes,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?;
        if valid_role_count != role_codes.len() as i64 {
            return Err(AccountMutationError::InvalidRole);
        }

        sqlx::query!(
            "DELETE FROM account_roles WHERE tenant_id = $1 AND account_id = $2",
            tenant_id,
            account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?;
        for role_code in &role_codes {
            sqlx::query!(
                r#"
                INSERT INTO account_roles (tenant_id, account_id, role_code, assigned_by_account_id)
                VALUES ($1, $2, $3, $4)
                "#,
                tenant_id,
                account_id,
                role_code,
                audit_account_id,
            )
            .execute(transaction.connection())
            .await
            .map_err(|_| AccountMutationError::BackendUnavailable)?;
        }
        sqlx::query!(
            r#"
            UPDATE accounts
            SET primary_role_code = $3,
                auth_version = auth_version + 1,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $4
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            account_id,
            primary_role.as_code(),
            audit_account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?;
        transaction.commit().await.map_err(|error| {
            log_error!(
                "Account role replacement commit failed: account_id={} error={}",
                account_id,
                error
            );
            AccountMutationError::BackendUnavailable
        })?;
        log_notice!(
            "Account roles replaced: tenant_id={} account_id={} primary_role={} role_count={} actor_account_id={}",
            tenant_id,
            account_id,
            primary_role.as_code(),
            role_codes.len(),
            audit_account_id
        );
        Ok(())
    }

    async fn replace_permissions(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        permissions: &[AccountPermission],
        audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        if permissions.len() > 256 {
            return Err(AccountMutationError::InvalidPermission);
        }
        let permission_codes: BTreeSet<String> = permissions
            .iter()
            .map(|permission| permission.code.trim().to_owned())
            .collect();
        if permission_codes.len() != permissions.len()
            || permission_codes
                .iter()
                .any(|permission| !permission_code_is_valid(permission))
        {
            return Err(AccountMutationError::InvalidPermission);
        }

        let mut transaction: TenantTransaction = self.mutation_transaction(tenant_id).await?;
        let account_exists: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM accounts WHERE tenant_id = $1 AND id = $2) AS "exists!""#,
            tenant_id,
            account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?;
        if !account_exists {
            return Err(AccountMutationError::AccountNotFound);
        }
        let permission_codes: Vec<String> = permission_codes.into_iter().collect();
        let valid_permission_count: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM permissions WHERE code = ANY($1)"#,
            &permission_codes,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?;
        if valid_permission_count != permission_codes.len() as i64 {
            return Err(AccountMutationError::InvalidPermission);
        }

        sqlx::query!(
            "DELETE FROM account_permissions WHERE tenant_id = $1 AND account_id = $2",
            tenant_id,
            account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?;
        for permission in permissions {
            sqlx::query!(
                r#"
                INSERT INTO account_permissions (
                    tenant_id, account_id, permission_code, effect, expires_at, granted_by_account_id
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
                tenant_id,
                account_id,
                permission.code.trim(),
                permission.effect.as_code(),
                permission.expires_at,
                audit_account_id,
            )
            .execute(transaction.connection())
            .await
            .map_err(|error| {
                log_warn!(
                    "Account permission override rejected: account_id={} permission={} error={}",
                    account_id,
                    permission.code,
                    error
                );
                AccountMutationError::InvalidPermission
            })?;
        }
        sqlx::query!(
            r#"
            UPDATE accounts
            SET auth_version = auth_version + 1,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $3
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            account_id,
            audit_account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|_| AccountMutationError::BackendUnavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| AccountMutationError::BackendUnavailable)?;
        log_notice!(
            "Account permission overrides replaced: tenant_id={} account_id={} override_count={} actor_account_id={}",
            tenant_id,
            account_id,
            permissions.len(),
            audit_account_id
        );
        Ok(())
    }
}

async fn ensure_another_active_owner(
    connection: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    account_id: Uuid,
) -> Result<(), AccountMutationError> {
    // Lock the complete owner set so concurrent demotions cannot both observe
    // another owner and leave the tenant without an administrator.
    let active_owner_ids: Vec<Uuid> = sqlx::query_scalar!(
        r#"
        SELECT id
        FROM accounts
        WHERE tenant_id = $1
          AND status = 'active'
          AND primary_role_code = 'tenant_owner'
        ORDER BY id
        FOR UPDATE
        "#,
        tenant_id,
    )
    .fetch_all(connection)
    .await
    .map_err(|_| AccountMutationError::BackendUnavailable)?;
    if !active_owner_ids.iter().any(|owner_id: &Uuid| *owner_id != account_id) {
        return Err(AccountMutationError::LastTenantOwner);
    }
    Ok(())
}

fn role_code_is_valid(value: &str) -> bool {
    (2..=63).contains(&value.len())
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| byte.is_ascii_lowercase() || (index > 0 && (byte.is_ascii_digit() || byte == b'_')))
}

fn permission_code_is_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.contains('.')
        && value.split('.').all(|segment: &str| {
            !segment.is_empty()
                && segment.bytes().enumerate().all(|(index, byte)| {
                    byte.is_ascii_lowercase() || (index > 0 && (byte.is_ascii_digit() || byte == b'_'))
                })
        })
}
