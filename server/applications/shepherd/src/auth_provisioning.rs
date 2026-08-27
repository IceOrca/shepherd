use async_trait::async_trait;
use infra_auth::ext_service::auth_admin::{
    AuthAccountAccessContext, AuthAccountProvisioner, AuthAccountProvisioningContext, AuthAccountProvisioningError,
};
use sqlx::{PgConnection, postgres::PgQueryResult};
use tracing::{debug, error, trace};
use uuid::Uuid;

const TENANT_OWNER_ROLE_CODE: &str = "tenant_owner";

#[derive(Debug)]
pub struct ShepherdAuthAccountProvisioner;

#[async_trait]
impl AuthAccountProvisioner for ShepherdAuthAccountProvisioner {
    async fn provision(
        &self,
        connection: &mut PgConnection,
        context: &AuthAccountProvisioningContext,
    ) -> Result<(), AuthAccountProvisioningError> {
        if context.primary_role.as_str() == TENANT_OWNER_ROLE_CODE {
            trace!(
                tenant_id = %context.tenant_id,
                account_id = %context.account_id,
                primary_role = %context.primary_role,
                "Skipped HR employee provisioning for the tenant-owner account"
            );
            return Ok(());
        }

        let branch_id: Uuid = employee_branch(
            context.tenant_id,
            context.actor_account_id,
            context.account_id,
            &context.branch_ids,
            None,
        )?;

        let employee_code: String = generated_employee_code(&context.username, context.account_id);
        let result: PgQueryResult = sqlx::query!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, branch_id, account_id, employee_code, display_name,
                status, hire_date, created_by_account_id, updated_by_account_id
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                'active', CURRENT_DATE, $7, $7
            )
            "#,
            uuid::Uuid::new_v4(),
            context.tenant_id,
            branch_id,
            context.account_id,
            employee_code,
            context.username,
            context.actor_account_id,
        )
        .execute(connection)
        .await
        .map_err(|database_error: sqlx::Error| {
            error!(
                tenant_id = %context.tenant_id,
                actor_id = %context.actor_account_id,
                account_id = %context.account_id,
                error = %database_error,
                "Automatic HR employee provisioning failed"
            );
            AuthAccountProvisioningError::new("hr_employee_insert_failed")
        })?;
        debug!(
            tenant_id = %context.tenant_id,
            actor_id = %context.actor_account_id,
            account_id = %context.account_id,
            branch_id = %branch_id,
            rows_affected = result.rows_affected(),
            "Active HR employee profile provisioned for Auth account"
        );
        Ok(())
    }

    async fn update_access(
        &self,
        connection: &mut PgConnection,
        context: &AuthAccountAccessContext,
    ) -> Result<(), AuthAccountProvisioningError> {
        if context.primary_role.as_str() == TENANT_OWNER_ROLE_CODE {
            let result: PgQueryResult = sqlx::query!(
                r#"
                UPDATE hr_employees
                SET account_id = NULL, updated_at = CURRENT_TIMESTAMP, updated_by_account_id = $3
                WHERE tenant_id = $1 AND account_id = $2
                "#,
                context.tenant_id,
                context.account_id,
                context.actor_account_id,
            )
            .execute(connection)
            .await
            .map_err(|database_error: sqlx::Error| {
                error!(
                    tenant_id = %context.tenant_id,
                    actor_id = %context.actor_account_id,
                    account_id = %context.account_id,
                    error = %database_error,
                    "Tenant-owner account could not be detached from an HR employee"
                );
                AuthAccountProvisioningError::new("tenant_owner_employee_detach_failed")
            })?;
            trace!(
                tenant_id = %context.tenant_id,
                account_id = %context.account_id,
                primary_role = %context.primary_role,
                rows_affected = result.rows_affected(),
                "Tenant-owner account has no linked HR employee"
            );
            return Ok(());
        }
        let current_branch_id: Option<Uuid> = sqlx::query_scalar!(
            "SELECT branch_id FROM hr_employees WHERE tenant_id = $1 AND account_id = $2 FOR UPDATE",
            context.tenant_id,
            context.account_id,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(|database_error: sqlx::Error| {
            error!(
                tenant_id = %context.tenant_id,
                actor_id = %context.actor_account_id,
                account_id = %context.account_id,
                error = %database_error,
                "Existing HR employee branch could not be loaded"
            );
            AuthAccountProvisioningError::new("hr_employee_branch_load_failed")
        })?;
        let branch_id: Uuid = employee_branch(
            context.tenant_id,
            context.actor_account_id,
            context.account_id,
            &context.branch_ids,
            current_branch_id,
        )?;
        let employee_code: String = generated_employee_code(&context.username, context.account_id);
        let result: PgQueryResult = sqlx::query!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, branch_id, account_id, employee_code, display_name,
                status, hire_date, created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'active', CURRENT_DATE, $7, $7)
            ON CONFLICT (tenant_id, account_id) DO UPDATE
            SET branch_id = EXCLUDED.branch_id,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = EXCLUDED.updated_by_account_id
            "#,
            Uuid::new_v4(),
            context.tenant_id,
            branch_id,
            context.account_id,
            employee_code,
            context.username,
            context.actor_account_id,
        )
        .execute(connection)
        .await
        .map_err(|database_error: sqlx::Error| {
            error!(
                tenant_id = %context.tenant_id,
                actor_id = %context.actor_account_id,
                account_id = %context.account_id,
                branch_id = %branch_id,
                error = %database_error,
                "Shepherd HR employee branch synchronization failed"
            );
            AuthAccountProvisioningError::new("hr_employee_branch_update_failed")
        })?;
        debug!(
            tenant_id = %context.tenant_id,
            actor_id = %context.actor_account_id,
            account_id = %context.account_id,
            branch_id = %branch_id,
            rows_affected = result.rows_affected(),
            "Shepherd HR employee branch synchronized after account access update"
        );
        Ok(())
    }
}

fn employee_branch(
    tenant_id: Uuid,
    actor_account_id: Uuid,
    account_id: Uuid,
    branch_ids: &[Uuid],
    current_branch_id: Option<Uuid>,
) -> Result<Uuid, AuthAccountProvisioningError> {
    let branch_id: Uuid = current_branch_id
        .filter(|branch_id: &Uuid| branch_ids.contains(branch_id))
        .or_else(|| infra_postgres::active_branch_id().filter(|branch_id: &Uuid| branch_ids.contains(branch_id)))
        .or_else(|| branch_ids.first().copied())
        .ok_or_else(|| {
            error!(
                tenant_id = %tenant_id,
                actor_id = %actor_account_id,
                account_id = %account_id,
                "Non-owner account lifecycle received no employee branch assignment"
            );
            AuthAccountProvisioningError::new("employee_branch_assignment_missing")
        })?;
    Ok(branch_id)
}

fn generated_employee_code(username: &str, account_id: uuid::Uuid) -> String {
    let normalized_prefix: String = username
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character: char| -> char {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .filter(|character: &char| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(44)
        .collect::<String>()
        .trim_matches(['-', '_'])
        .to_owned();
    let safe_prefix: String = if normalized_prefix.len() >= 2 {
        normalized_prefix
    } else {
        "employee".to_owned()
    };
    let reversed_account_suffix: String = account_id.simple().to_string().chars().rev().take(12).collect();
    let account_suffix: String = reversed_account_suffix.chars().rev().collect();
    format!("{safe_prefix}-{account_suffix}")
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{employee_branch, generated_employee_code};

    #[test]
    fn generated_employee_codes_are_normalized_bounded_and_account_specific() -> Result<(), uuid::Error> {
        let first_account_id: Uuid = Uuid::parse_str("00000000-0000-4000-8000-000000000001")?;
        let second_account_id: Uuid = Uuid::parse_str("00000000-0000-4000-8000-000000000002")?;
        let first_code: String = generated_employee_code(" Nguyễn Văn A / Night Shift ", first_account_id);
        let second_code: String = generated_employee_code(" Nguyễn Văn A / Night Shift ", second_account_id);

        assert!(first_code.len() <= 63);
        assert!(first_code.ends_with("-000000000001"));
        assert_ne!(first_code, second_code);
        assert!(first_code.chars().all(|character: char| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '-' | '_')
        }));
        Ok(())
    }

    #[test]
    fn employee_branch_preserves_current_authorized_branch_for_multi_branch_roles() {
        let first_branch: Uuid = Uuid::new_v4();
        let current_branch: Uuid = Uuid::new_v4();
        let selected: Uuid = employee_branch(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            &[first_branch, current_branch],
            Some(current_branch),
        )
        .unwrap_or_else(|error| panic!("employee branch must resolve: {error}"));
        assert_eq!(selected, current_branch);
    }
}
