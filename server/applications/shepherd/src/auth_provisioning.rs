use async_trait::async_trait;
use infra_auth::ext_foundation::auth_admin::{
    AuthAccountProvisioner, AuthAccountProvisioningContext, AuthAccountProvisioningError,
};
use sqlx::{PgConnection, postgres::PgQueryResult};
use tracing::{debug, error, trace};

const STAFF_ROLE_CODE: &str = "staff";

#[derive(Debug)]
pub struct ShepherdAuthAccountProvisioner;

#[async_trait]
impl AuthAccountProvisioner for ShepherdAuthAccountProvisioner {
    async fn provision(
        &self,
        connection: &mut PgConnection,
        context: &AuthAccountProvisioningContext,
    ) -> Result<(), AuthAccountProvisioningError> {
        if context.primary_role.as_str() != STAFF_ROLE_CODE {
            trace!(
                tenant_id = %context.tenant_id,
                account_id = %context.account_id,
                primary_role = %context.primary_role,
                "Skipped HR employee provisioning for a non-staff account"
            );
            return Ok(());
        }

        let branch_id: uuid::Uuid = context.branch_ids.first().copied().ok_or_else(|| {
            error!(
                tenant_id = %context.tenant_id,
                actor_id = %context.actor_account_id,
                account_id = %context.account_id,
                "Staff account provisioning received no branch assignment"
            );
            AuthAccountProvisioningError::new("staff_branch_assignment_missing")
        })?;
        if context.branch_ids.len() != 1 {
            error!(
                tenant_id = %context.tenant_id,
                actor_id = %context.actor_account_id,
                account_id = %context.account_id,
                branch_count = context.branch_ids.len(),
                "Staff account provisioning received multiple branch assignments"
            );
            return Err(AuthAccountProvisioningError::new("staff_branch_assignment_invalid"));
        }

        let employee_code: String = generated_employee_code(&context.username, context.account_id);
        let result: PgQueryResult = sqlx::query!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, branch_id, account_id, employee_code, display_name, work_email,
                status, hire_date, created_by_account_id, updated_by_account_id
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                'active', CURRENT_DATE, $8, $8
            )
            "#,
            uuid::Uuid::new_v4(),
            context.tenant_id,
            branch_id,
            context.account_id,
            employee_code,
            context.username,
            context.email,
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

    use super::generated_employee_code;

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
}
