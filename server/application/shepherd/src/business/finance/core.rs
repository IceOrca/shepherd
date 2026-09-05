use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

use tracing::{debug, error, info, trace, warn};
use super::database::FinanceRepo;

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseClaimStatus {
    Submitted,
    Approved,
    Rejected,
    Cancelled,
}

impl ExpenseClaimStatus {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

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
}

impl SalaryAdvanceRecoverySource {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::ManualRepayment => "manual_repayment",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
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
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Approved => "approved",
            Self::Disbursed => "disbursed",
            Self::Recovered => "recovered",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

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
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
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
    pub revision_id: Uuid,
    pub revision_number: i64,
    pub revision_kind: String,
    pub correction_reason: Option<String>,
    pub revised_by_username: String,
    pub revised_at: DateTime<Utc>,
    pub financial_period_open: bool,
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
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
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
pub struct ExpenseCorrectionInput {
    pub expected_revision_id: Uuid,
    pub correction_reason: String,
    pub category_id: Uuid,
    pub funding_source: ExpenseFundingSource,
    pub paid_by_employee_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub urgent_work_report_id: Option<Uuid>,
    pub staffing_assignment_id: Option<Uuid>,
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
    pub description: String,
    pub evidence_reference: Option<String>,
    pub claimed_amount: String,
    pub approved_amount: Option<String>,
    pub currency: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct ExpenseClaimRevision {
    pub revision_id: Uuid,
    pub revision_number: i64,
    pub revision_kind: String,
    pub correction_reason: Option<String>,
    pub revised_by_username: String,
    pub revised_at: DateTime<Utc>,
    pub category_name: String,
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
    pub description: String,
    pub claimed_amount: String,
    pub approved_amount: Option<String>,
    pub currency: String,
    pub status: ExpenseClaimStatus,
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
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
    pub status: SalaryAdvanceStatus,
    pub decision_reason: Option<String>,
    pub requested_by_username: String,
    pub approved_by_username: Option<String>,
    pub disbursed_by_username: Option<String>,
    pub disbursement_reference: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub disbursed_at: Option<DateTime<Utc>>,
    pub revision_id: Uuid,
    pub revision_number: i64,
    pub revision_kind: String,
    pub correction_reason: Option<String>,
    pub revised_by_username: String,
    pub revised_at: DateTime<Utc>,
    pub financial_period_open: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct SalaryAdvanceInput {
    pub employee_id: Uuid,
    pub requested_amount: String,
    pub currency: String,
    pub reason: String,
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
}

#[derive(Clone, Debug)]
pub struct SalaryAdvanceDecisionInput {
    pub approved_amount: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SalaryAdvanceCorrectionInput {
    pub expected_revision_id: Uuid,
    pub correction_reason: String,
    pub employee_id: Uuid,
    pub requested_amount: String,
    pub approved_amount: Option<String>,
    pub currency: String,
    pub reason: String,
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct SalaryAdvanceRevision {
    pub revision_id: Uuid,
    pub revision_number: i64,
    pub revision_kind: String,
    pub correction_reason: Option<String>,
    pub revised_by_username: String,
    pub revised_at: DateTime<Utc>,
    pub employee_name: String,
    pub requested_amount: String,
    pub approved_amount: Option<String>,
    pub currency: String,
    pub reason: String,
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
    pub status: SalaryAdvanceStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExpenseCursor {
    pub paid_on: NaiveDate,
    pub created_at: DateTime<Utc>,
    pub expense_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct ExpensePage {
    pub items: Vec<ExpenseClaim>,
    pub next_cursor: Option<ExpenseCursor>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SalaryAdvanceCursor {
    pub requested_at: DateTime<Utc>,
    pub advance_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct SalaryAdvancePage {
    pub items: Vec<SalaryAdvance>,
    pub next_cursor: Option<SalaryAdvanceCursor>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RevisionCursor {
    pub revision_number: i64,
}

#[derive(Clone, Debug)]
pub struct ExpenseRevisionPage {
    pub items: Vec<ExpenseClaimRevision>,
    pub next_cursor: Option<RevisionCursor>,
}

#[derive(Clone, Debug)]
pub struct SalaryAdvanceRevisionPage {
    pub items: Vec<SalaryAdvanceRevision>,
    pub next_cursor: Option<RevisionCursor>,
}

pub struct ExpenseListQuery {
    pub status: Option<ExpenseClaimStatus>,
    pub search: Option<String>,
    pub limit: i64,
    pub cursor: Option<ExpenseCursor>,
}

pub struct SalaryAdvanceListQuery {
    pub status: Option<SalaryAdvanceStatus>,
    pub search: Option<String>,
    pub limit: i64,
    pub cursor: Option<SalaryAdvanceCursor>,
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

pub struct FinanceService {
    repo: Arc<FinanceRepo>,
}

impl FinanceService {
    pub fn new_arc(repo: Arc<FinanceRepo>) -> Arc<Self> {
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
        query: ExpenseListQuery,
    ) -> Result<ExpensePage, FinanceError> {
        validate_page_limit(query.limit)?;
        self.repo
            .list_expenses(tenant_id, actor_account_id, can_read_all, &query)
            .await
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
        validate_payroll_inclusion_date(input.paid_on, input.payroll_inclusion_on)?;
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

    pub async fn correct_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        can_correct_confirmed: bool,
        idempotency_key: Uuid,
        input: ExpenseCorrectionInput,
    ) -> Result<ExpenseClaim, FinanceError> {
        validate_uuid(expense_id)?;
        validate_uuid(input.expected_revision_id)?;
        validate_uuid(input.category_id)?;
        validate_text(&input.correction_reason, 3, 500, "correction reason is invalid")?;
        validate_context_values(input.urgent_work_report_id, input.staffing_assignment_id)?;
        validate_payroll_inclusion_date(input.paid_on, input.payroll_inclusion_on)?;
        validate_money(&input.claimed_amount, &input.currency)?;
        if let Some(approved_amount) = input.approved_amount.as_deref() {
            validate_positive_decimal(approved_amount)?;
        }
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
            .correct_expense(
                tenant_id,
                expense_id,
                actor_account_id,
                can_correct_confirmed,
                idempotency_key,
                &input,
            )
            .await
    }

    pub async fn list_expense_revisions(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
        limit: i64,
        cursor: Option<RevisionCursor>,
    ) -> Result<ExpenseRevisionPage, FinanceError> {
        validate_uuid(expense_id)?;
        validate_page_limit(limit)?;
        self.repo
            .list_expense_revisions(
                tenant_id,
                expense_id,
                actor_account_id,
                can_read_all,
                limit,
                cursor.as_ref(),
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
        query: SalaryAdvanceListQuery,
    ) -> Result<SalaryAdvancePage, FinanceError> {
        validate_page_limit(query.limit)?;
        self.repo
            .list_salary_advances(tenant_id, actor_account_id, can_read_all, &query)
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
        validate_payroll_inclusion_date(input.paid_on, input.payroll_inclusion_on)?;
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

    pub async fn correct_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        can_manage_requested: bool,
        can_correct_confirmed: bool,
        idempotency_key: Uuid,
        input: SalaryAdvanceCorrectionInput,
    ) -> Result<SalaryAdvance, FinanceError> {
        validate_uuid(advance_id)?;
        validate_uuid(input.expected_revision_id)?;
        validate_uuid(input.employee_id)?;
        validate_text(&input.correction_reason, 3, 500, "correction reason is invalid")?;
        validate_payroll_inclusion_date(input.paid_on, input.payroll_inclusion_on)?;
        validate_money(&input.requested_amount, &input.currency)?;
        if let Some(approved_amount) = input.approved_amount.as_deref() {
            validate_positive_decimal(approved_amount)?;
        }
        validate_text(&input.reason, 3, 500, "salary advance reason is invalid")?;
        self.repo
            .correct_salary_advance(
                tenant_id,
                advance_id,
                actor_account_id,
                can_manage_requested,
                can_correct_confirmed,
                idempotency_key,
                &input,
            )
            .await
    }

    pub async fn list_salary_advance_revisions(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
        limit: i64,
        cursor: Option<RevisionCursor>,
    ) -> Result<SalaryAdvanceRevisionPage, FinanceError> {
        validate_uuid(advance_id)?;
        validate_page_limit(limit)?;
        self.repo
            .list_salary_advance_revisions(
                tenant_id,
                advance_id,
                actor_account_id,
                can_read_all,
                limit,
                cursor.as_ref(),
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

fn validate_page_limit(limit: i64) -> Result<(), FinanceError> {
    if limit <= 0 {
        Err(FinanceError::InvalidInput("finance page size must be positive"))
    } else {
        Ok(())
    }
}

fn validate_context(input: &ExpenseClaimInput) -> Result<(), FinanceError> {
    validate_context_values(input.urgent_work_report_id, input.staffing_assignment_id)
}

fn validate_context_values(
    urgent_work_report_id: Option<Uuid>,
    staffing_assignment_id: Option<Uuid>,
) -> Result<(), FinanceError> {
    let specific_contexts: usize = [urgent_work_report_id, staffing_assignment_id]
        .into_iter()
        .flatten()
        .count();
    if specific_contexts > 1 {
        return Err(FinanceError::InvalidInput("expense can reference only one work record"));
    }
    Ok(())
}

fn validate_payroll_inclusion_date(paid_on: NaiveDate, payroll_inclusion_on: NaiveDate) -> Result<(), FinanceError> {
    if payroll_inclusion_on < paid_on {
        Err(FinanceError::InvalidInput(
            "payroll inclusion date cannot be before paid date",
        ))
    } else {
        Ok(())
    }
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
    use chrono::NaiveDate;

    use super::{validate_payroll_inclusion_date, validate_positive_decimal};

    #[test]
    fn financial_amounts_are_exact_bounded_decimal_strings() {
        assert!(validate_positive_decimal("250000.0000").is_ok());
        assert!(validate_positive_decimal("0").is_err());
        assert!(validate_positive_decimal("12.12345").is_err());
        assert!(validate_positive_decimal("-100").is_err());
    }

    #[test]
    fn payroll_inclusion_date_is_not_before_payment_date() {
        let paid_on: NaiveDate = NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid test date");
        let later_period: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 1).expect("valid test date");
        let earlier_period: NaiveDate = NaiveDate::from_ymd_opt(2026, 8, 31).expect("valid test date");

        assert!(validate_payroll_inclusion_date(paid_on, paid_on).is_ok());
        assert!(validate_payroll_inclusion_date(paid_on, later_period).is_ok());
        assert!(validate_payroll_inclusion_date(paid_on, earlier_period).is_err());
    }
}
