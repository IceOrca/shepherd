use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseFundingSource {
    CompanyFunds,
    EmployeePersonal,
}

impl ExpenseFundingSource {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::CompanyFunds => "company_funds",
            Self::EmployeePersonal => "employee_personal",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "company_funds" => Some(Self::CompanyFunds),
            "employee_personal" => Some(Self::EmployeePersonal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseClaimStatus {
    Submitted,
    Approved,
    Rejected,
    Cancelled,
}

impl ExpenseClaimStatus {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "submitted" => Some(Self::Submitted),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SalaryAdvanceRecoverySource {
    ManualRepayment,
    PayrollDeduction,
}

impl SalaryAdvanceRecoverySource {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::ManualRepayment => "manual_repayment",
            Self::PayrollDeduction => "payroll_deduction",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SalaryAdvanceStatus {
    Requested,
    Approved,
    Disbursed,
    Recovered,
    Rejected,
    Cancelled,
}

impl SalaryAdvanceStatus {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "requested" => Some(Self::Requested),
            "approved" => Some(Self::Approved),
            "disbursed" => Some(Self::Disbursed),
            "recovered" => Some(Self::Recovered),
            "rejected" => Some(Self::Rejected),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct ExpenseCategory {
    pub id: Uuid,
    pub code: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct ExpenseClaim {
    pub id: Uuid,
    pub branch_id: Uuid,
    pub category_id: Uuid,
    pub category_name: String,
    pub funding_source: ExpenseFundingSource,
    pub paid_by_employee_id: Option<Uuid>,
    pub paid_by_employee_name: Option<String>,
    pub customer_id: Option<Uuid>,
    pub urgent_work_report_id: Option<Uuid>,
    pub staffing_assignment_id: Option<Uuid>,
    pub incurred_on: NaiveDate,
    pub description: String,
    pub evidence_reference: Option<String>,
    pub claimed_amount: String,
    pub approved_amount: Option<String>,
    pub reimbursed_amount: String,
    pub outstanding_reimbursement: String,
    pub currency: String,
    pub status: ExpenseClaimStatus,
    pub decision_reason: Option<String>,
    pub submitted_by_account_id: Uuid,
    pub submitted_by_username: String,
    pub approved_by_username: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ExpenseClaimInput {
    pub category_id: Uuid,
    pub funding_source: ExpenseFundingSource,
    pub paid_by_employee_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub urgent_work_report_id: Option<Uuid>,
    pub staffing_assignment_id: Option<Uuid>,
    pub incurred_on: NaiveDate,
    pub description: String,
    pub evidence_reference: Option<String>,
    pub claimed_amount: String,
    pub currency: String,
}

#[derive(Clone, Debug)]
pub struct ExpenseDecisionInput {
    pub approved_amount: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FinancialSettlementInput {
    pub amount: String,
    pub reference: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct SalaryAdvance {
    pub id: Uuid,
    pub branch_id: Uuid,
    pub employee_id: Uuid,
    pub employee_code: String,
    pub employee_name: String,
    pub requested_amount: String,
    pub approved_amount: Option<String>,
    pub recovered_amount: String,
    pub outstanding_amount: String,
    pub currency: String,
    pub reason: String,
    pub recovery_due_on: Option<NaiveDate>,
    pub status: SalaryAdvanceStatus,
    pub decision_reason: Option<String>,
    pub requested_by_username: String,
    pub approved_by_username: Option<String>,
    pub disbursed_by_username: Option<String>,
    pub disbursement_reference: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub disbursed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct SalaryAdvanceInput {
    pub employee_id: Uuid,
    pub requested_amount: String,
    pub currency: String,
    pub reason: String,
    pub recovery_due_on: Option<NaiveDate>,
}

#[derive(Clone, Debug)]
pub struct SalaryAdvanceDecisionInput {
    pub approved_amount: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SalaryAdvanceRecoveryInput {
    pub amount: String,
    pub source: SalaryAdvanceRecoverySource,
    pub reference: String,
}

#[derive(Clone, Debug)]
pub struct FinancialDecisionCommand {
    pub actor_account_id: Uuid,
    pub idempotency_key: Uuid,
    pub approved: bool,
    pub approved_amount: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Error)]
pub enum FinanceError {
    #[error("invalid financial input: {0}")]
    InvalidInput(&'static str),
    #[error("financial record was not found")]
    NotFound,
    #[error("financial workflow conflict")]
    Conflict,
    #[error("financial action is forbidden")]
    Forbidden,
    #[error("financial storage is unavailable")]
    BackendUnavailable,
}

#[async_trait]
pub trait FinanceRepo: Send + Sync {
    async fn list_expense_categories(&self, tenant_id: Uuid) -> Result<Vec<ExpenseCategory>, FinanceError>;
    async fn list_expenses(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
    ) -> Result<Vec<ExpenseClaim>, FinanceError>;
    async fn create_expense(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_submit_for_others: bool,
        idempotency_key: Uuid,
        input: &ExpenseClaimInput,
    ) -> Result<ExpenseClaim, FinanceError>;
    async fn decide_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        command: &FinancialDecisionCommand,
    ) -> Result<ExpenseClaim, FinanceError>;
    async fn reimburse_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &FinancialSettlementInput,
    ) -> Result<ExpenseClaim, FinanceError>;
    async fn list_salary_advances(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
    ) -> Result<Vec<SalaryAdvance>, FinanceError>;
    async fn create_salary_advance(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_request_for_others: bool,
        idempotency_key: Uuid,
        input: &SalaryAdvanceInput,
    ) -> Result<SalaryAdvance, FinanceError>;
    async fn decide_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        command: &FinancialDecisionCommand,
    ) -> Result<SalaryAdvance, FinanceError>;
    async fn disburse_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        reference: &str,
    ) -> Result<SalaryAdvance, FinanceError>;
    async fn recover_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &SalaryAdvanceRecoveryInput,
    ) -> Result<SalaryAdvance, FinanceError>;
}

pub struct FinanceService {
    repo: Arc<dyn FinanceRepo>,
}

impl FinanceService {
    pub fn new_arc(repo: Arc<dyn FinanceRepo>) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_expense_categories(&self, tenant_id: Uuid) -> Result<Vec<ExpenseCategory>, FinanceError> {
        self.repo.list_expense_categories(tenant_id).await
    }

    pub async fn list_expenses(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
    ) -> Result<Vec<ExpenseClaim>, FinanceError> {
        self.repo.list_expenses(tenant_id, actor_account_id, can_read_all).await
    }

    pub async fn create_expense(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_submit_for_others: bool,
        idempotency_key: Uuid,
        input: ExpenseClaimInput,
    ) -> Result<ExpenseClaim, FinanceError> {
        validate_uuid(input.category_id)?;
        validate_context(&input)?;
        validate_money(&input.claimed_amount, &input.currency)?;
        validate_text(&input.description, 3, 1000, "expense description is invalid")?;
        validate_optional_text(
            input.evidence_reference.as_deref(),
            1,
            500,
            "expense evidence is invalid",
        )?;
        match input.funding_source {
            ExpenseFundingSource::CompanyFunds if input.paid_by_employee_id.is_some() => {
                return Err(FinanceError::InvalidInput(
                    "company-funded expense cannot have an employee payer",
                ));
            }
            ExpenseFundingSource::EmployeePersonal if input.paid_by_employee_id.is_none() => {
                return Err(FinanceError::InvalidInput("employee-paid expense requires a payer"));
            }
            _ => {}
        }
        self.repo
            .create_expense(
                tenant_id,
                actor_account_id,
                can_submit_for_others,
                idempotency_key,
                &input,
            )
            .await
    }

    pub async fn approve_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: ExpenseDecisionInput,
    ) -> Result<ExpenseClaim, FinanceError> {
        validate_uuid(expense_id)?;
        validate_positive_decimal(&input.approved_amount)?;
        validate_optional_text(input.reason.as_deref(), 3, 500, "decision reason is invalid")?;
        self.repo
            .decide_expense(
                tenant_id,
                expense_id,
                &FinancialDecisionCommand {
                    actor_account_id,
                    idempotency_key,
                    approved: true,
                    approved_amount: Some(input.approved_amount),
                    reason: input.reason,
                },
            )
            .await
    }

    pub async fn reject_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        reason: &str,
    ) -> Result<ExpenseClaim, FinanceError> {
        validate_text(reason, 3, 500, "rejection reason is invalid")?;
        self.repo
            .decide_expense(
                tenant_id,
                expense_id,
                &FinancialDecisionCommand {
                    actor_account_id,
                    idempotency_key,
                    approved: false,
                    approved_amount: None,
                    reason: Some(reason.to_owned()),
                },
            )
            .await
    }

    pub async fn reimburse_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: FinancialSettlementInput,
    ) -> Result<ExpenseClaim, FinanceError> {
        validate_positive_decimal(&input.amount)?;
        validate_text(&input.reference, 3, 500, "payment reference is invalid")?;
        self.repo
            .reimburse_expense(tenant_id, expense_id, actor_account_id, idempotency_key, &input)
            .await
    }

    pub async fn list_salary_advances(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
    ) -> Result<Vec<SalaryAdvance>, FinanceError> {
        self.repo
            .list_salary_advances(tenant_id, actor_account_id, can_read_all)
            .await
    }

    pub async fn create_salary_advance(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_request_for_others: bool,
        idempotency_key: Uuid,
        input: SalaryAdvanceInput,
    ) -> Result<SalaryAdvance, FinanceError> {
        validate_uuid(input.employee_id)?;
        validate_money(&input.requested_amount, &input.currency)?;
        validate_text(&input.reason, 3, 500, "salary advance reason is invalid")?;
        self.repo
            .create_salary_advance(
                tenant_id,
                actor_account_id,
                can_request_for_others,
                idempotency_key,
                &input,
            )
            .await
    }

    pub async fn approve_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: SalaryAdvanceDecisionInput,
    ) -> Result<SalaryAdvance, FinanceError> {
        validate_positive_decimal(&input.approved_amount)?;
        validate_optional_text(input.reason.as_deref(), 3, 500, "decision reason is invalid")?;
        self.repo
            .decide_salary_advance(
                tenant_id,
                advance_id,
                &FinancialDecisionCommand {
                    actor_account_id,
                    idempotency_key,
                    approved: true,
                    approved_amount: Some(input.approved_amount),
                    reason: input.reason,
                },
            )
            .await
    }

    pub async fn reject_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        reason: &str,
    ) -> Result<SalaryAdvance, FinanceError> {
        validate_text(reason, 3, 500, "rejection reason is invalid")?;
        self.repo
            .decide_salary_advance(
                tenant_id,
                advance_id,
                &FinancialDecisionCommand {
                    actor_account_id,
                    idempotency_key,
                    approved: false,
                    approved_amount: None,
                    reason: Some(reason.to_owned()),
                },
            )
            .await
    }

    pub async fn disburse_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        reference: &str,
    ) -> Result<SalaryAdvance, FinanceError> {
        validate_text(reference, 3, 500, "disbursement reference is invalid")?;
        self.repo
            .disburse_salary_advance(tenant_id, advance_id, actor_account_id, idempotency_key, reference)
            .await
    }

    pub async fn recover_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: SalaryAdvanceRecoveryInput,
    ) -> Result<SalaryAdvance, FinanceError> {
        validate_positive_decimal(&input.amount)?;
        validate_text(&input.reference, 3, 500, "recovery reference is invalid")?;
        self.repo
            .recover_salary_advance(tenant_id, advance_id, actor_account_id, idempotency_key, &input)
            .await
    }
}

fn validate_uuid(value: Uuid) -> Result<(), FinanceError> {
    if value.is_nil() {
        Err(FinanceError::InvalidInput("identifier is invalid"))
    } else {
        Ok(())
    }
}

fn validate_context(input: &ExpenseClaimInput) -> Result<(), FinanceError> {
    let specific_contexts: usize = [input.urgent_work_report_id, input.staffing_assignment_id]
        .into_iter()
        .flatten()
        .count();
    if specific_contexts > 1 {
        return Err(FinanceError::InvalidInput("expense can reference only one work record"));
    }
    Ok(())
}

fn validate_money(amount: &str, currency: &str) -> Result<(), FinanceError> {
    validate_positive_decimal(amount)?;
    if currency.len() != 3 || currency.chars().any(|character: char| !character.is_ascii_uppercase()) {
        return Err(FinanceError::InvalidInput(
            "currency must be a three-letter uppercase code",
        ));
    }
    Ok(())
}

fn validate_positive_decimal(value: &str) -> Result<(), FinanceError> {
    let mut parts: std::str::Split<'_, char> = value.split('.');
    let whole: &str = parts.next().unwrap_or_default();
    let fraction: Option<&str> = parts.next();
    let zero: bool = value
        .chars()
        .all(|character: char| character == '0' || character == '.');
    if value.is_empty()
        || value != value.trim()
        || value.starts_with('-')
        || parts.next().is_some()
        || whole.is_empty()
        || whole.len() > 15
        || !whole.chars().all(|character: char| character.is_ascii_digit())
        || fraction.is_some_and(|part: &str| {
            part.is_empty() || part.len() > 4 || !part.chars().all(|character: char| character.is_ascii_digit())
        })
        || zero
    {
        return Err(FinanceError::InvalidInput("money amount is invalid"));
    }
    Ok(())
}

fn validate_text(value: &str, minimum: usize, maximum: usize, message: &'static str) -> Result<(), FinanceError> {
    if value != value.trim() || value.chars().count() < minimum || value.chars().count() > maximum {
        Err(FinanceError::InvalidInput(message))
    } else {
        Ok(())
    }
}

fn validate_optional_text(
    value: Option<&str>,
    minimum: usize,
    maximum: usize,
    message: &'static str,
) -> Result<(), FinanceError> {
    value.map_or(Ok(()), |value: &str| validate_text(value, minimum, maximum, message))
}

#[cfg(test)]
mod tests {
    use super::validate_positive_decimal;

    #[test]
    fn financial_amounts_are_exact_bounded_decimal_strings() {
        assert!(validate_positive_decimal("250000.0000").is_ok());
        assert!(validate_positive_decimal("0").is_err());
        assert!(validate_positive_decimal("12.12345").is_err());
        assert!(validate_positive_decimal("-100").is_err());
    }
}
