use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::auth::RoleCode;

use super::super::core::FinanceError;

#[derive(Clone, Debug, Serialize, TS)]
pub struct EmployeeSalaryConfig {
    pub employee_id: Uuid,
    pub branch_id: Uuid,
    pub employee_code: String,
    pub employee_name: String,
    pub role: RoleCode,
    pub rate_id: Option<Uuid>,
    pub monthly_amount: Option<String>,
    pub currency: Option<String>,
    pub effective_from: Option<NaiveDate>,
    pub effective_to: Option<NaiveDate>,
}

#[derive(Clone, Debug)]
pub struct EmployeeSalaryRateInput {
    pub employee_id: Uuid,
    pub monthly_amount: String,
    pub currency: String,
    pub effective_from: NaiveDate,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FinancialPeriodStatus {
    Open,
    Closed,
}

impl FinancialPeriodStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct FinancialPeriodState {
    pub branch_id: Uuid,
    pub period_start: NaiveDate,
    pub status: FinancialPeriodStatus,
    pub revision_number: i64,
    pub reason: Option<String>,
    pub actor_username: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct FinancialPeriodChangeInput {
    pub period_start: NaiveDate,
    pub status: FinancialPeriodStatus,
    pub expected_revision_number: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct OperatingFinancialLine {
    pub currency: String,
    pub staffing_revenue: String,
    pub staffing_worker_cost: String,
    pub coordination_salary_cost: String,
    pub approved_business_expense: String,
    pub profit_share_cost: String,
    pub operating_cost: String,
    pub operating_profit: String,
    pub business_profit_after_profit_share: String,
    pub reimbursed_cash: String,
    pub salary_advance_disbursed: String,
    pub salary_advance_recovered: String,
    pub outstanding_expense_reimbursement: String,
    pub outstanding_salary_advance: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct OperatingFinancialReport {
    pub branch_id: Uuid,
    pub branch_name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub lines: Vec<OperatingFinancialLine>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct PayrollLine {
    pub employee_id: Uuid,
    pub branch_id: Uuid,
    pub employee_code: String,
    pub employee_name: String,
    pub role: RoleCode,
    pub currency: String,
    pub staffing_worked_seconds: i64,
    pub staffing_earnings: String,
    pub prorated_monthly_salary: String,
    pub profit_share_base: String,
    pub profit_share_percent: String,
    pub profit_share_payment: String,
    pub profit_share_locked: bool,
    pub gross_pay: String,
    pub recorded_expense_reimbursement: String,
    pub suggested_expense_reimbursement: String,
    pub recorded_advance_deduction: String,
    pub outstanding_advance_due: String,
    pub suggested_advance_deduction: String,
    pub estimated_net_pay: String,
    pub attendance_overlap_count: i64,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct PayrollReport {
    pub branch_id: Uuid,
    pub branch_name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub lines: Vec<PayrollLine>,
}

#[async_trait]
pub trait FinancialReportingRepo: Send + Sync {
    async fn list_financial_periods(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<FinancialPeriodState>, FinanceError>;

    async fn change_financial_period(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &FinancialPeriodChangeInput,
    ) -> Result<FinancialPeriodState, FinanceError>;

    async fn list_salary_configurations(&self, tenant_id: Uuid) -> Result<Vec<EmployeeSalaryConfig>, FinanceError>;

    async fn create_salary_rate(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &EmployeeSalaryRateInput,
    ) -> Result<EmployeeSalaryConfig, FinanceError>;

    async fn operating_report(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<OperatingFinancialReport, FinanceError>;

    async fn payroll_report(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<PayrollReport, FinanceError>;
}

pub struct FinancialReportingService {
    repo: Arc<dyn FinancialReportingRepo>,
}

impl FinancialReportingService {
    pub fn new_arc(repo: Arc<dyn FinancialReportingRepo>) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_salary_configurations(&self, tenant_id: Uuid) -> Result<Vec<EmployeeSalaryConfig>, FinanceError> {
        self.repo.list_salary_configurations(tenant_id).await
    }

    pub async fn list_financial_periods(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<FinancialPeriodState>, FinanceError> {
        validate_range(start_date, end_date)?;
        self.repo.list_financial_periods(tenant_id, start_date, end_date).await
    }

    pub async fn change_financial_period(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: FinancialPeriodChangeInput,
    ) -> Result<FinancialPeriodState, FinanceError> {
        if input.period_start.day() != 1 {
            return Err(FinanceError::InvalidInput(
                "financial period must start on the first day of a month",
            ));
        }
        if input.expected_revision_number < 0 {
            return Err(FinanceError::InvalidInput(
                "financial period revision cannot be negative",
            ));
        }
        validate_text(&input.reason, 3, 500, "financial period reason is invalid")?;
        self.repo
            .change_financial_period(tenant_id, actor_account_id, idempotency_key, &input)
            .await
    }

    pub async fn create_salary_rate(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: EmployeeSalaryRateInput,
    ) -> Result<EmployeeSalaryConfig, FinanceError> {
        validate_uuid(input.employee_id)?;
        validate_positive_decimal(&input.monthly_amount)?;
        if input.currency.len() != 3
            || input
                .currency
                .chars()
                .any(|character: char| !character.is_ascii_uppercase())
        {
            return Err(FinanceError::InvalidInput(
                "currency must be a three-letter uppercase code",
            ));
        }
        self.repo
            .create_salary_rate(tenant_id, actor_account_id, idempotency_key, &input)
            .await
    }

    pub async fn operating_report(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<OperatingFinancialReport, FinanceError> {
        validate_range(start_date, end_date)?;
        self.repo.operating_report(tenant_id, start_date, end_date).await
    }

    pub async fn payroll_report(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<PayrollReport, FinanceError> {
        validate_range(start_date, end_date)?;
        self.repo.payroll_report(tenant_id, start_date, end_date).await
    }
}

fn validate_range(start_date: NaiveDate, end_date: NaiveDate) -> Result<(), FinanceError> {
    let days: i64 = end_date.signed_duration_since(start_date).num_days();
    if !(0..=3_660).contains(&days) {
        return Err(FinanceError::InvalidInput("report date range is invalid or too large"));
    }
    Ok(())
}

fn validate_uuid(value: Uuid) -> Result<(), FinanceError> {
    if value.is_nil() {
        Err(FinanceError::InvalidInput("identifier cannot be nil"))
    } else {
        Ok(())
    }
}

fn validate_positive_decimal(value: &str) -> Result<(), FinanceError> {
    let mut parts = value.split('.');
    let whole: &str = parts.next().unwrap_or_default();
    let fraction: Option<&str> = parts.next();
    let is_zero: bool = value
        .chars()
        .all(|character: char| character == '0' || character == '.');
    if value.is_empty()
        || value != value.trim()
        || value.starts_with('-')
        || parts.next().is_some()
        || whole.is_empty()
        || whole.len() > 15
        || !whole.chars().all(|character: char| character.is_ascii_digit())
        || fraction.is_some_and(|digits: &str| {
            digits.is_empty() || digits.len() > 4 || !digits.chars().all(|character: char| character.is_ascii_digit())
        })
        || is_zero
    {
        return Err(FinanceError::InvalidInput("amount must be a positive decimal"));
    }
    Ok(())
}

fn validate_text(value: &str, minimum: usize, maximum: usize, message: &'static str) -> Result<(), FinanceError> {
    let length: usize = value.chars().count();
    if value != value.trim() || length < minimum || length > maximum {
        Err(FinanceError::InvalidInput(message))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{validate_positive_decimal, validate_range};

    #[test]
    fn report_ranges_are_bounded_and_inclusive() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date");
        assert!(validate_range(start, start).is_ok());
        assert!(validate_range(start, NaiveDate::from_ymd_opt(2025, 12, 31).expect("valid date")).is_err());
    }

    #[test]
    fn salary_amounts_use_exact_decimal_strings() {
        assert!(validate_positive_decimal("15000000.0000").is_ok());
        assert!(validate_positive_decimal("0").is_err());
        assert!(validate_positive_decimal("12.12345").is_err());
    }
}
