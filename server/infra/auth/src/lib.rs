#![cfg_attr(debug_assertions, allow(unused))]

pub mod access_revocation;
pub mod account;
pub mod bruteforce;
pub mod dto;
mod feature;
pub mod handler;
pub mod jwks;
pub mod jwt;
pub mod middleware;
pub mod postgres;
pub mod route;
pub mod service;
pub mod session;
pub mod token_blacklist_pubsub;
pub mod typescript;

pub use feature::{AuthFeature, AuthService, AuthenticatedUser, TenantContext};
pub use postgres::AuthProvider;

use std::sync::Arc;

use async_trait::async_trait;
use infra_kernel::{
    debug::{log_error, log_notice},
    security::{hash_passphrase, verify_passphrase},
};
use uuid::Uuid;

use account::{AccountPermission, AccountStatus, AccountSummary, AuthorizationCatalog, Role, UserAccount};

#[derive(Debug)]
pub enum StoreAccountError {
    UsernameAlreadyExists,
    BackendUnavailable,
}

#[derive(Debug)]
pub enum AccountMutationError {
    AccountNotFound,
    InvalidRole,
    InvalidPermission,
    LastTenantOwner,
    BackendUnavailable,
}

#[async_trait]
pub trait AccountRepo {
    async fn resolve_active_tenant_id(&self, tenant: &str) -> Result<Option<Uuid>, String>;
    async fn find_by_username(&self, tenant_id: Uuid, username: &str) -> Result<Option<UserAccount>, String>;
    async fn store(
        &self,
        tenant_id: Uuid,
        account: &UserAccount,
        audit_account_id: Option<Uuid>,
    ) -> Result<(), StoreAccountError>;
    async fn list_accounts(&self, _tenant_id: Uuid) -> Result<Vec<AccountSummary>, String> {
        Err("account listing is not implemented".to_owned())
    }
    async fn list_authorization_catalog(&self, _tenant_id: Uuid) -> Result<AuthorizationCatalog, String> {
        Err("authorization catalog is not implemented".to_owned())
    }
    async fn mark_authenticated(&self, _tenant_id: Uuid, _account_id: Uuid) -> Result<(), String> {
        Err("authentication audit update is not implemented".to_owned())
    }
    async fn update_password(
        &self,
        _tenant_id: Uuid,
        _account_id: Uuid,
        _passphrase_key: &str,
        _audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        Err(AccountMutationError::BackendUnavailable)
    }
    async fn update_status(
        &self,
        _tenant_id: Uuid,
        _account_id: Uuid,
        _status: AccountStatus,
        _audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        Err(AccountMutationError::BackendUnavailable)
    }
    async fn replace_roles(
        &self,
        _tenant_id: Uuid,
        _account_id: Uuid,
        _primary_role: Role,
        _roles: &[String],
        _audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        Err(AccountMutationError::BackendUnavailable)
    }
    async fn replace_permissions(
        &self,
        _tenant_id: Uuid,
        _account_id: Uuid,
        _permissions: &[AccountPermission],
        _audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        Err(AccountMutationError::BackendUnavailable)
    }
}

pub type DynAccountRepo = Arc<dyn AccountRepo + Send + Sync>;

#[derive(Debug)]
pub enum AuthenticateUserError {
    InvalidCredentials(Option<Role>),
    BackendUnavailable,
}

#[derive(Debug)]
pub enum CreateAccountError {
    UsernameAlreadyExists,
    BackendUnavailable,
}

#[derive(Debug)]
pub enum ChangeOwnPasswordError {
    InvalidCurrentPassword,
    AccountNotFound,
    BackendUnavailable,
}

pub struct AuthMngtEntity {
    pub acct_auth_ops: DynAccountRepo,
    dummy_passphrase_key: String,
}

impl AuthMngtEntity {
    pub async fn new_arc(acct_auth_ops: DynAccountRepo) -> Arc<Self> {
        let dummy_passphrase_key: String =
            hash_passphrase("infra-auth-dummy-credential").unwrap_or_else(|error: String| {
                log_error!("Failed to initialize dummy authentication passphrase key: {}", error);
                panic!("Failed to initialize dummy authentication passphrase key");
            });

        Arc::new(Self {
            acct_auth_ops,
            dummy_passphrase_key,
        })
    }

    pub async fn get_current_account_by_username(
        &self,
        tenant_id: Uuid,
        username: &str,
    ) -> Result<Option<UserAccount>, String> {
        self.acct_auth_ops
            .find_by_username(tenant_id, username)
            .await
            .map_err(|error: String| {
                log_error!(
                    "Error fetching current account state for tenant={} username='{}': {}",
                    tenant_id,
                    username,
                    error
                );
                error
            })
    }

    pub async fn authenticate_user_for_tenant(
        &self,
        tenant: &str,
        username: &str,
        passphrase_plain: &str,
    ) -> Result<(Uuid, UserAccount), AuthenticateUserError> {
        let tenant_id: Uuid = match self.acct_auth_ops.resolve_active_tenant_id(tenant).await {
            Ok(Some(tenant_id)) => tenant_id,
            Ok(None) => {
                let _dummy_result: bool = verify_passphrase(passphrase_plain, &self.dummy_passphrase_key);
                return Err(AuthenticateUserError::InvalidCredentials(None));
            }
            Err(error) => {
                log_error!("Error resolving active tenant slug='{}': {}", tenant, error);
                return Err(AuthenticateUserError::BackendUnavailable);
            }
        };

        self.authenticate_user(tenant_id, username, passphrase_plain)
            .await
            .map(|account: UserAccount| (tenant_id, account))
    }

    pub async fn authenticate_user(
        &self,
        tenant_id: Uuid,
        username: &str,
        passphrase_plain: &str,
    ) -> Result<UserAccount, AuthenticateUserError> {
        let account: UserAccount = match self.get_current_account_by_username(tenant_id, username).await {
            Ok(Some(account)) => account,
            Ok(None) => {
                let _dummy_result: bool = verify_passphrase(passphrase_plain, &self.dummy_passphrase_key);
                return Err(AuthenticateUserError::InvalidCredentials(None));
            }
            Err(_) => return Err(AuthenticateUserError::BackendUnavailable),
        };

        let passphrase_valid: bool = verify_passphrase(passphrase_plain, &account.passphrase_key);
        if !account.active || !passphrase_valid {
            return Err(AuthenticateUserError::InvalidCredentials(Some(account.role.clone())));
        }

        self.acct_auth_ops
            .mark_authenticated(tenant_id, account.id)
            .await
            .map_err(|error: String| {
                log_error!(
                    "Failed to persist successful-login audit: tenant={} account={} error={}",
                    tenant_id,
                    account.id,
                    error
                );
                AuthenticateUserError::BackendUnavailable
            })?;

        Ok(account)
    }

    pub async fn create_account(
        &self,
        tenant_id: Uuid,
        username: &str,
        passphrase_plain: &str,
        role: Role,
        audit_account_id: Option<Uuid>,
    ) -> Result<UserAccount, CreateAccountError> {
        if self
            .get_current_account_by_username(tenant_id, username)
            .await
            .map_err(|_| CreateAccountError::BackendUnavailable)?
            .is_some()
        {
            return Err(CreateAccountError::UsernameAlreadyExists);
        }

        let passphrase_key: String = hash_passphrase(passphrase_plain).map_err(|error: String| {
            log_error!("Failed to hash passphrase for '{}': {}", username, error);
            CreateAccountError::BackendUnavailable
        })?;
        let new_account = UserAccount {
            id: Uuid::new_v4(),
            username: username.to_owned(),
            passphrase_key,
            roles: vec![role.as_code().to_owned()],
            role,
            active: true,
            auth_version: 1,
            permissions: Vec::new(),
        };

        self.acct_auth_ops
            .store(tenant_id, &new_account, audit_account_id)
            .await
            .map_err(|error: StoreAccountError| match error {
                StoreAccountError::UsernameAlreadyExists => CreateAccountError::UsernameAlreadyExists,
                StoreAccountError::BackendUnavailable => CreateAccountError::BackendUnavailable,
            })?;

        log_notice!(
            "New account '{}' registered in tenant={} by account={:?}",
            username,
            tenant_id,
            audit_account_id
        );
        Ok(new_account)
    }

    pub async fn list_accounts(&self, tenant_id: Uuid) -> Result<Vec<AccountSummary>, String> {
        self.acct_auth_ops.list_accounts(tenant_id).await
    }

    pub async fn list_authorization_catalog(&self, tenant_id: Uuid) -> Result<AuthorizationCatalog, String> {
        self.acct_auth_ops.list_authorization_catalog(tenant_id).await
    }

    pub async fn change_own_password(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        username: &str,
        current_passphrase: &str,
        new_passphrase: &str,
    ) -> Result<(), ChangeOwnPasswordError> {
        let account: UserAccount = self
            .get_current_account_by_username(tenant_id, username)
            .await
            .map_err(|_| ChangeOwnPasswordError::BackendUnavailable)?
            .filter(|account: &UserAccount| account.id == account_id)
            .ok_or(ChangeOwnPasswordError::AccountNotFound)?;
        if !verify_passphrase(current_passphrase, &account.passphrase_key) {
            return Err(ChangeOwnPasswordError::InvalidCurrentPassword);
        }

        self.set_password(tenant_id, account_id, new_passphrase, account_id)
            .await
            .map_err(|error: AccountMutationError| match error {
                AccountMutationError::AccountNotFound => ChangeOwnPasswordError::AccountNotFound,
                _ => ChangeOwnPasswordError::BackendUnavailable,
            })
    }

    pub async fn set_password(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        new_passphrase: &str,
        audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        let passphrase_key: String =
            hash_passphrase(new_passphrase).map_err(|_| AccountMutationError::BackendUnavailable)?;
        self.acct_auth_ops
            .update_password(tenant_id, account_id, &passphrase_key, audit_account_id)
            .await
    }

    pub async fn set_account_status(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        status: AccountStatus,
        audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        self.acct_auth_ops
            .update_status(tenant_id, account_id, status, audit_account_id)
            .await
    }

    pub async fn set_account_roles(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        primary_role: Role,
        roles: &[String],
        audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        self.acct_auth_ops
            .replace_roles(tenant_id, account_id, primary_role, roles, audit_account_id)
            .await
    }

    pub async fn set_account_permissions(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        permissions: &[AccountPermission],
        audit_account_id: Uuid,
    ) -> Result<(), AccountMutationError> {
        self.acct_auth_ops
            .replace_permissions(tenant_id, account_id, permissions, audit_account_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::{AccountRepo, AuthMngtEntity, AuthenticateUserError, CreateAccountError, DynAccountRepo, StoreAccountError};
    use crate::account::{Role, UserAccount};

    struct DuplicateDuringStoreRepo;

    #[async_trait]
    impl AccountRepo for DuplicateDuringStoreRepo {
        async fn resolve_active_tenant_id(&self, _tenant: &str) -> Result<Option<Uuid>, String> {
            Ok(None)
        }

        async fn find_by_username(&self, _tenant_id: Uuid, _username: &str) -> Result<Option<UserAccount>, String> {
            Ok(None)
        }

        async fn store(
            &self,
            _tenant_id: Uuid,
            _account: &UserAccount,
            _audit_account_id: Option<Uuid>,
        ) -> Result<(), StoreAccountError> {
            Err(StoreAccountError::UsernameAlreadyExists)
        }
    }

    #[tokio::test]
    async fn maps_duplicate_detected_during_insert_to_username_conflict() {
        let repository: DynAccountRepo = Arc::new(DuplicateDuringStoreRepo);
        let authentication: Arc<AuthMngtEntity> = AuthMngtEntity::new_arc(repository).await;
        let result = authentication
            .create_account(
                Uuid::new_v4(),
                "alice",
                "valid-password",
                Role::Employee,
                Some(Uuid::new_v4()),
            )
            .await;

        assert!(matches!(result, Err(CreateAccountError::UsernameAlreadyExists)));
    }

    #[tokio::test]
    async fn unknown_tenant_slug_is_reported_as_invalid_credentials() {
        let repository: DynAccountRepo = Arc::new(DuplicateDuringStoreRepo);
        let authentication: Arc<AuthMngtEntity> = AuthMngtEntity::new_arc(repository).await;

        let result = authentication
            .authenticate_user_for_tenant("missing", "alice", "valid-password")
            .await;

        assert!(matches!(result, Err(AuthenticateUserError::InvalidCredentials(None))));
    }
}
