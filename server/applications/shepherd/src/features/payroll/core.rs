//! HR payroll compensation, premium rules, and monthly calculation domain.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PayBasis {
    Hourly,
    Monthly,
}

impl PayBasis {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::Monthly => "monthly",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "hourly" => Some(Self::Hourly),
            "monthly" => Some(Self::Monthly),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PayrollRunStatus {
    Draft,
    Calculated,
    Approved,
    Paid,
}

impl PayrollRunStatus {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "draft" => Some(Self::Draft),
            "calculated" => Some(Self::Calculated),
            "approved" => Some(Self::Approved),
            "paid" => Some(Self::Paid),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct EmployeeCompensation {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub currency: String,
    pub pay_basis: PayBasis,
    pub hourly_rate: Option<String>,
    pub monthly_rate: Option<String>,
    pub standard_monthly_hours: Option<String>,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct EmployeeCompensationInput {
    pub currency: String,
    pub pay_basis: PayBasis,
    pub hourly_rate: Option<String>,
    pub monthly_rate: Option<String>,
    pub standard_monthly_hours: Option<String>,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct BranchRateRule {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub branch_id: Uuid,
    pub employee_id: Option<Uuid>,
    pub base_multiplier: String,
    pub hourly_adjustment: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

#[derive(Clone, Debug)]
pub struct BranchRateRuleInput {
    pub code: String,
    pub name: String,
    pub branch_id: Uuid,
    pub employee_id: Option<Uuid>,
    pub base_multiplier: String,
    pub hourly_adjustment: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct TimeBandRule {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub weekdays: Vec<i16>,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub spans_next_day: bool,
    pub premium_multiplier: String,
    pub hourly_adjustment: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

#[derive(Clone, Debug)]
pub struct TimeBandRuleInput {
    pub code: String,
    pub name: String,
    pub weekdays: Vec<i16>,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub spans_next_day: bool,
    pub premium_multiplier: String,
    pub hourly_adjustment: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct OvertimeRule {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub threshold_minutes: i32,
    pub premium_multiplier: String,
    pub hourly_adjustment: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

#[derive(Clone, Debug)]
pub struct OvertimeRuleInput {
    pub code: String,
    pub name: String,
    pub threshold_minutes: i32,
    pub premium_multiplier: String,
    pub hourly_adjustment: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct PayrollEmployeeResult {
    pub employee_id: Uuid,
    pub worked_seconds: i64,
    pub base_amount: String,
    pub branch_amount: String,
    pub time_amount: String,
    pub overtime_amount: String,
    pub gross_amount: String,
    pub currency: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct PayrollLine {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub attendance_session_id: Option<Uuid>,
    pub staffing_assignment_id: Option<Uuid>,
    pub branch_id: Uuid,
    pub work_date: NaiveDate,
    pub component: String,
    pub rule_code: Option<String>,
    pub worked_seconds: i64,
    pub base_hourly_rate: String,
    pub multiplier: String,
    pub hourly_adjustment: String,
    pub amount: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct PayrollRun {
    pub id: Uuid,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub time_zone: String,
    pub currency: String,
    pub status: PayrollRunStatus,
    pub calculated_at: Option<DateTime<Utc>>,
    pub approved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub results: Vec<PayrollEmployeeResult>,
    pub lines: Vec<PayrollLine>,
}

#[derive(Clone, Debug)]
pub struct PayrollRunInput {
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub time_zone: String,
    pub currency: String,
}

#[derive(Debug)]
pub enum PayrollError {
    NotFound,
    Conflict,
    InvalidInput(&'static str),
    MissingCompensation,
    OverlappingWorkSources,
    BackendUnavailable,
}

#[async_trait]
pub trait PayrollRepo {
    async fn list_compensations(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<EmployeeCompensation>, PayrollError>;
    async fn create_compensation(
        &self,
        tenant_id: Uuid,
        compensation_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeCompensationInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeCompensation, PayrollError>;
    async fn list_branch_rules(&self, tenant_id: Uuid) -> Result<Vec<BranchRateRule>, PayrollError>;
    async fn create_branch_rule(
        &self,
        tenant_id: Uuid,
        rule_id: Uuid,
        input: &BranchRateRuleInput,
        audit_account_id: Uuid,
    ) -> Result<BranchRateRule, PayrollError>;
    async fn list_time_band_rules(&self, tenant_id: Uuid) -> Result<Vec<TimeBandRule>, PayrollError>;
    async fn create_time_band_rule(
        &self,
        tenant_id: Uuid,
        rule_id: Uuid,
        input: &TimeBandRuleInput,
        audit_account_id: Uuid,
    ) -> Result<TimeBandRule, PayrollError>;
    async fn list_overtime_rules(&self, tenant_id: Uuid) -> Result<Vec<OvertimeRule>, PayrollError>;
    async fn create_overtime_rule(
        &self,
        tenant_id: Uuid,
        rule_id: Uuid,
        input: &OvertimeRuleInput,
        audit_account_id: Uuid,
    ) -> Result<OvertimeRule, PayrollError>;
    async fn list_runs(&self, tenant_id: Uuid) -> Result<Vec<PayrollRun>, PayrollError>;
    async fn find_run(&self, tenant_id: Uuid, payroll_run_id: Uuid) -> Result<Option<PayrollRun>, PayrollError>;
    async fn calculate_run(
        &self,
        tenant_id: Uuid,
        payroll_run_id: Uuid,
        input: &PayrollRunInput,
        audit_account_id: Uuid,
    ) -> Result<PayrollRun, PayrollError>;
    async fn approve_run(
        &self,
        tenant_id: Uuid,
        payroll_run_id: Uuid,
        audit_account_id: Uuid,
    ) -> Result<PayrollRun, PayrollError>;
}

pub type DynPayrollRepo = Arc<dyn PayrollRepo + Send + Sync>;

pub struct PayrollService {
    repo: DynPayrollRepo,
}

impl PayrollService {
    pub fn new_arc(repo: DynPayrollRepo) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_compensations(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<EmployeeCompensation>, PayrollError> {
        self.repo.list_compensations(tenant_id, employee_id).await
    }

    pub async fn create_compensation(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: EmployeeCompensationInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeCompensation, PayrollError> {
        validate_compensation(&input)?;
        self.repo
            .create_compensation(tenant_id, Uuid::new_v4(), employee_id, &input, audit_account_id)
            .await
    }

    pub async fn list_branch_rules(&self, tenant_id: Uuid) -> Result<Vec<BranchRateRule>, PayrollError> {
        self.repo.list_branch_rules(tenant_id).await
    }

    pub async fn create_branch_rule(
        &self,
        tenant_id: Uuid,
        input: BranchRateRuleInput,
        audit_account_id: Uuid,
    ) -> Result<BranchRateRule, PayrollError> {
        validate_rule_identity(&input.code, &input.name, input.effective_from, input.effective_to)?;
        validate_decimal(&input.base_multiplier, false)?;
        if !decimal_at_least_one(&input.base_multiplier) {
            return Err(PayrollError::InvalidInput(
                "branch base multiplier must be at least one",
            ));
        }
        validate_decimal(&input.hourly_adjustment, true)?;
        self.repo
            .create_branch_rule(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn list_time_band_rules(&self, tenant_id: Uuid) -> Result<Vec<TimeBandRule>, PayrollError> {
        self.repo.list_time_band_rules(tenant_id).await
    }

    pub async fn create_time_band_rule(
        &self,
        tenant_id: Uuid,
        input: TimeBandRuleInput,
        audit_account_id: Uuid,
    ) -> Result<TimeBandRule, PayrollError> {
        validate_rule_identity(&input.code, &input.name, input.effective_from, input.effective_to)?;
        if input.weekdays.is_empty()
            || input.weekdays.len() > 7
            || input.weekdays.iter().any(|weekday: &i16| !(1..=7).contains(weekday))
        {
            return Err(PayrollError::InvalidInput("time rule weekdays are invalid"));
        }
        if input.start_time == input.end_time
            || (!input.spans_next_day && input.end_time <= input.start_time)
            || (input.spans_next_day && input.end_time > input.start_time)
        {
            return Err(PayrollError::InvalidInput("time rule range is invalid"));
        }
        validate_decimal(&input.premium_multiplier, true)?;
        validate_decimal(&input.hourly_adjustment, true)?;
        if is_zero_decimal(&input.premium_multiplier) && is_zero_decimal(&input.hourly_adjustment) {
            return Err(PayrollError::InvalidInput("time rule has no premium"));
        }
        self.repo
            .create_time_band_rule(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn list_overtime_rules(&self, tenant_id: Uuid) -> Result<Vec<OvertimeRule>, PayrollError> {
        self.repo.list_overtime_rules(tenant_id).await
    }

    pub async fn create_overtime_rule(
        &self,
        tenant_id: Uuid,
        input: OvertimeRuleInput,
        audit_account_id: Uuid,
    ) -> Result<OvertimeRule, PayrollError> {
        validate_rule_identity(&input.code, &input.name, input.effective_from, input.effective_to)?;
        if input.threshold_minutes <= 0 {
            return Err(PayrollError::InvalidInput("overtime threshold must be positive"));
        }
        validate_decimal(&input.premium_multiplier, true)?;
        validate_decimal(&input.hourly_adjustment, true)?;
        if is_zero_decimal(&input.premium_multiplier) && is_zero_decimal(&input.hourly_adjustment) {
            return Err(PayrollError::InvalidInput("overtime rule has no premium"));
        }
        self.repo
            .create_overtime_rule(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn list_runs(&self, tenant_id: Uuid) -> Result<Vec<PayrollRun>, PayrollError> {
        self.repo.list_runs(tenant_id).await
    }

    pub async fn find_run(&self, tenant_id: Uuid, payroll_run_id: Uuid) -> Result<Option<PayrollRun>, PayrollError> {
        self.repo.find_run(tenant_id, payroll_run_id).await
    }

    pub async fn calculate_month(
        &self,
        tenant_id: Uuid,
        year: i32,
        month: u32,
        time_zone: String,
        currency: String,
        audit_account_id: Uuid,
    ) -> Result<PayrollRun, PayrollError> {
        if !(1970..=9998).contains(&year) || !(1..=12).contains(&month) {
            return Err(PayrollError::InvalidInput("payroll month is invalid"));
        }
        let period_start: NaiveDate =
            NaiveDate::from_ymd_opt(year, month, 1).ok_or(PayrollError::InvalidInput("payroll month is invalid"))?;
        let (next_year, next_month): (i32, u32) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
        let period_end: NaiveDate = NaiveDate::from_ymd_opt(next_year, next_month, 1)
            .ok_or(PayrollError::InvalidInput("payroll month is invalid"))?;
        let time_zone: String = time_zone.trim().to_owned();
        let currency: String = currency.trim().to_ascii_uppercase();
        if time_zone.is_empty() || time_zone.len() > 128 {
            return Err(PayrollError::InvalidInput("payroll time zone is invalid"));
        }
        validate_currency(&currency)?;
        self.repo
            .calculate_run(
                tenant_id,
                Uuid::new_v4(),
                &PayrollRunInput {
                    period_start,
                    period_end,
                    time_zone,
                    currency,
                },
                audit_account_id,
            )
            .await
    }

    pub async fn approve_run(
        &self,
        tenant_id: Uuid,
        payroll_run_id: Uuid,
        audit_account_id: Uuid,
    ) -> Result<PayrollRun, PayrollError> {
        self.repo.approve_run(tenant_id, payroll_run_id, audit_account_id).await
    }
}

fn validate_compensation(input: &EmployeeCompensationInput) -> Result<(), PayrollError> {
    validate_currency(&input.currency)?;
    validate_dates(input.effective_from, input.effective_to)?;
    match input.pay_basis {
        PayBasis::Hourly => {
            let hourly_rate: &str = input
                .hourly_rate
                .as_deref()
                .ok_or(PayrollError::InvalidInput("hourly compensation requires hourly_rate"))?;
            validate_decimal(hourly_rate, false)?;
            if input.monthly_rate.is_some() || input.standard_monthly_hours.is_some() {
                return Err(PayrollError::InvalidInput("hourly compensation has monthly values"));
            }
        }
        PayBasis::Monthly => {
            let monthly_rate: &str = input
                .monthly_rate
                .as_deref()
                .ok_or(PayrollError::InvalidInput("monthly compensation requires monthly_rate"))?;
            let monthly_hours: &str = input
                .standard_monthly_hours
                .as_deref()
                .ok_or(PayrollError::InvalidInput(
                    "monthly compensation requires standard hours",
                ))?;
            validate_decimal(monthly_rate, false)?;
            validate_decimal(monthly_hours, false)?;
            if input.hourly_rate.is_some() {
                return Err(PayrollError::InvalidInput("monthly compensation has hourly_rate"));
            }
        }
    }
    Ok(())
}

fn validate_rule_identity(
    code: &str,
    name: &str,
    effective_from: NaiveDate,
    effective_to: Option<NaiveDate>,
) -> Result<(), PayrollError> {
    let valid_boundary: bool = code
        .chars()
        .next()
        .zip(code.chars().last())
        .is_some_and(|(first, last)| first.is_ascii_alphanumeric() && last.is_ascii_alphanumeric());
    if code.len() < 2
        || code.len() > 63
        || code != code.trim()
        || !valid_boundary
        || code
            .chars()
            .any(|value: char| !(value.is_ascii_lowercase() || value.is_ascii_digit() || value == '_' || value == '-'))
    {
        return Err(PayrollError::InvalidInput("payroll rule code is invalid"));
    }
    if name.is_empty() || name.len() > 200 || name != name.trim() {
        return Err(PayrollError::InvalidInput("payroll rule name is invalid"));
    }
    validate_dates(effective_from, effective_to)
}

fn validate_dates(effective_from: NaiveDate, effective_to: Option<NaiveDate>) -> Result<(), PayrollError> {
    if effective_to.is_some_and(|value: NaiveDate| value < effective_from) {
        return Err(PayrollError::InvalidInput("effective date range is invalid"));
    }
    Ok(())
}

fn validate_currency(currency: &str) -> Result<(), PayrollError> {
    if currency.len() != 3 || currency.chars().any(|value: char| !value.is_ascii_uppercase()) {
        return Err(PayrollError::InvalidInput(
            "currency must be a three-letter uppercase code",
        ));
    }
    Ok(())
}

fn validate_decimal(value: &str, allow_zero: bool) -> Result<(), PayrollError> {
    let normalized: &str = value.trim();
    if normalized.is_empty() || normalized != value || normalized.starts_with('-') {
        return Err(PayrollError::InvalidInput("decimal amount is invalid"));
    }
    let mut parts = normalized.split('.');
    let whole: &str = parts
        .next()
        .ok_or(PayrollError::InvalidInput("decimal amount is invalid"))?;
    let fraction: Option<&str> = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || whole.chars().any(|value: char| !value.is_ascii_digit())
        || fraction.is_some_and(|part: &str| {
            part.is_empty() || part.len() > 4 || !part.chars().all(|value| value.is_ascii_digit())
        })
        || (!allow_zero && is_zero_decimal(normalized))
    {
        return Err(PayrollError::InvalidInput("decimal amount is invalid"));
    }
    Ok(())
}

fn is_zero_decimal(value: &str) -> bool {
    value
        .chars()
        .all(|character: char| character == '0' || character == '.')
}

fn decimal_at_least_one(value: &str) -> bool {
    value
        .split('.')
        .next()
        .is_some_and(|whole: &str| whole.chars().any(|digit: char| digit != '0'))
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{EmployeeCompensationInput, PayBasis, decimal_at_least_one, validate_compensation, validate_rule_identity};

    #[test]
    fn monthly_compensation_requires_a_divisor() {
        let input = EmployeeCompensationInput {
            currency: "THB".to_owned(),
            pay_basis: PayBasis::Monthly,
            hourly_rate: None,
            monthly_rate: Some("30000.0000".to_owned()),
            standard_monthly_hours: None,
            effective_from: NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date"),
            effective_to: None,
        };
        assert!(validate_compensation(&input).is_err());
    }

    #[test]
    fn hourly_compensation_accepts_four_decimal_places() {
        let input = EmployeeCompensationInput {
            currency: "THB".to_owned(),
            pay_basis: PayBasis::Hourly,
            hourly_rate: Some("125.5000".to_owned()),
            monthly_rate: None,
            standard_monthly_hours: None,
            effective_from: NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date"),
            effective_to: None,
        };
        assert!(validate_compensation(&input).is_ok());
    }

    #[test]
    fn branch_multiplier_cannot_reduce_base_pay() {
        assert!(!decimal_at_least_one("0.9999"));
        assert!(decimal_at_least_one("1.0000"));
        assert!(decimal_at_least_one("12.5000"));
    }

    #[test]
    fn rule_code_requires_alphanumeric_boundaries() {
        let effective_from: NaiveDate = NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date");
        assert!(validate_rule_identity("night-shift", "Night shift", effective_from, None).is_ok());
        assert!(validate_rule_identity("-night", "Night shift", effective_from, None).is_err());
        assert!(validate_rule_identity("night_", "Night shift", effective_from, None).is_err());
    }
}
