use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveTime, Timelike, Utc};
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::features::people::core::{HrError, HrRecordStatus, validate_code_and_name};

const MINUTES_PER_DAY: u32 = 24 * 60;
const MINUTES_PER_WEEK: u32 = 7 * MINUTES_PER_DAY;
const MAX_PERIODS_PER_SCHEDULE: usize = 64;

#[derive(Clone, Debug, Serialize, TS)]
pub struct WorkingPeriod {
    pub id: Uuid,
    pub weekday: i16,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub spans_next_day: bool,
    pub unpaid_break_minutes: i16,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct WorkingSchedule {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub time_zone: String,
    pub status: HrRecordStatus,
    pub periods: Vec<WorkingPeriod>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct WorkingPeriodInput {
    pub weekday: i16,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub spans_next_day: bool,
    pub unpaid_break_minutes: i16,
}

#[derive(Clone, Debug)]
pub struct WorkingScheduleInput {
    pub code: String,
    pub name: String,
    pub time_zone: String,
    pub status: HrRecordStatus,
    pub periods: Vec<WorkingPeriodInput>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct EmployeeScheduleAssignment {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub schedule_id: Uuid,
    pub date_start: NaiveDate,
    pub date_end: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct EmployeeScheduleAssignmentInput {
    pub schedule_id: Uuid,
    pub date_start: NaiveDate,
    pub date_end: Option<NaiveDate>,
}

#[async_trait]
pub trait WorkingScheduleRepo {
    async fn list_schedules(&self, tenant_id: Uuid) -> Result<Vec<WorkingSchedule>, HrError>;
    async fn find_schedule(&self, tenant_id: Uuid, schedule_id: Uuid) -> Result<Option<WorkingSchedule>, HrError>;
    async fn create_schedule(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        input: &WorkingScheduleInput,
        audit_account_id: Uuid,
    ) -> Result<WorkingSchedule, HrError>;
    async fn update_schedule(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        input: &WorkingScheduleInput,
        audit_account_id: Uuid,
    ) -> Result<WorkingSchedule, HrError>;
    async fn list_employee_assignments(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<EmployeeScheduleAssignment>, HrError>;
    async fn create_employee_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeScheduleAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeScheduleAssignment, HrError>;
}

pub type DynWorkingScheduleRepo = Arc<dyn WorkingScheduleRepo + Send + Sync>;

pub struct WorkingScheduleService {
    repo: DynWorkingScheduleRepo,
}

impl WorkingScheduleService {
    pub fn new_arc(repo: DynWorkingScheduleRepo) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list(&self, tenant_id: Uuid) -> Result<Vec<WorkingSchedule>, HrError> {
        self.repo.list_schedules(tenant_id).await
    }

    pub async fn find(&self, tenant_id: Uuid, schedule_id: Uuid) -> Result<Option<WorkingSchedule>, HrError> {
        self.repo.find_schedule(tenant_id, schedule_id).await
    }

    pub async fn create(
        &self,
        tenant_id: Uuid,
        input: WorkingScheduleInput,
        audit_account_id: Uuid,
    ) -> Result<WorkingSchedule, HrError> {
        validate_schedule(&input)?;
        self.repo
            .create_schedule(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn update(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        input: WorkingScheduleInput,
        audit_account_id: Uuid,
    ) -> Result<WorkingSchedule, HrError> {
        validate_schedule(&input)?;
        self.repo
            .update_schedule(tenant_id, schedule_id, &input, audit_account_id)
            .await
    }

    pub async fn list_employee_assignments(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<EmployeeScheduleAssignment>, HrError> {
        self.repo.list_employee_assignments(tenant_id, employee_id).await
    }

    pub async fn assign_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: EmployeeScheduleAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeScheduleAssignment, HrError> {
        if input.date_end.is_some_and(|date_end| date_end < input.date_start) {
            return Err(HrError::InvalidInput(
                "working schedule assignment end date precedes start date",
            ));
        }
        self.repo
            .create_employee_assignment(tenant_id, Uuid::new_v4(), employee_id, &input, audit_account_id)
            .await
    }
}

fn validate_schedule(input: &WorkingScheduleInput) -> Result<(), HrError> {
    validate_code_and_name(&input.code, &input.name)?;
    if input.time_zone.is_empty() || input.time_zone.len() > 128 || input.time_zone != input.time_zone.trim() {
        return Err(HrError::InvalidInput("time zone format is invalid"));
    }
    if input.periods.is_empty() {
        return Err(HrError::InvalidInput("a working schedule requires at least one period"));
    }
    if input.periods.len() > MAX_PERIODS_PER_SCHEDULE {
        return Err(HrError::InvalidInput("working schedule has too many periods"));
    }

    let mut intervals: Vec<(u32, u32)> = Vec::with_capacity(input.periods.len() + 1);
    for period in &input.periods {
        validate_period(period)?;
        let day_start: u32 = u32::try_from(period.weekday - 1)
            .map_err(|_| HrError::InvalidInput("working period weekday is invalid"))?
            * MINUTES_PER_DAY;
        let start: u32 = day_start + minutes_after_midnight(period.start_time);
        let end: u32 = if period.spans_next_day {
            day_start + MINUTES_PER_DAY + minutes_after_midnight(period.end_time)
        } else {
            day_start + minutes_after_midnight(period.end_time)
        };

        if end > MINUTES_PER_WEEK {
            intervals.push((start, MINUTES_PER_WEEK));
            intervals.push((0, end - MINUTES_PER_WEEK));
        } else {
            intervals.push((start, end));
        }
    }

    intervals.sort_unstable_by_key(|(start, _end): &(u32, u32)| *start);
    let mut previous_end: Option<u32> = None;
    for (start, end) in intervals {
        if previous_end.is_some_and(|value: u32| start < value) {
            return Err(HrError::InvalidInput("working schedule periods overlap"));
        }
        previous_end = Some(end);
    }
    Ok(())
}

fn validate_period(period: &WorkingPeriodInput) -> Result<(), HrError> {
    if !(1..=7).contains(&period.weekday) {
        return Err(HrError::InvalidInput("working period weekday is invalid"));
    }
    if period.start_time.second() != 0
        || period.start_time.nanosecond() != 0
        || period.end_time.second() != 0
        || period.end_time.nanosecond() != 0
    {
        return Err(HrError::InvalidInput("working period times must use whole minutes"));
    }
    if period.start_time == period.end_time
        || (!period.spans_next_day && period.end_time <= period.start_time)
        || (period.spans_next_day && period.end_time > period.start_time)
    {
        return Err(HrError::InvalidInput("working period time range is invalid"));
    }

    let start: u32 = minutes_after_midnight(period.start_time);
    let end: u32 = minutes_after_midnight(period.end_time);
    let duration: u32 = if period.spans_next_day {
        MINUTES_PER_DAY - start + end
    } else {
        end - start
    };
    let unpaid_break_minutes: u32 = u32::try_from(period.unpaid_break_minutes)
        .map_err(|_| HrError::InvalidInput("unpaid break duration is invalid"))?;
    if unpaid_break_minutes >= duration {
        return Err(HrError::InvalidInput(
            "unpaid break must be shorter than the working period",
        ));
    }
    Ok(())
}

fn minutes_after_midnight(time: NaiveTime) -> u32 {
    time.num_seconds_from_midnight() / 60
}

#[cfg(test)]
mod tests {
    use chrono::NaiveTime;

    use super::{WorkingPeriodInput, WorkingScheduleInput, validate_schedule};
    use crate::features::people::core::{HrError, HrRecordStatus};

    fn time(hour: u32, minute: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hour, minute, 0).expect("valid test time")
    }

    fn schedule(periods: Vec<WorkingPeriodInput>) -> WorkingScheduleInput {
        WorkingScheduleInput {
            code: "standard-40".to_owned(),
            name: "Standard 40 Hours".to_owned(),
            time_zone: "Asia/Bangkok".to_owned(),
            status: HrRecordStatus::Active,
            periods,
        }
    }

    #[test]
    fn accepts_non_overlapping_day_and_overnight_periods() {
        let result = validate_schedule(&schedule(vec![
            WorkingPeriodInput {
                weekday: 1,
                start_time: time(8, 0),
                end_time: time(17, 0),
                spans_next_day: false,
                unpaid_break_minutes: 60,
            },
            WorkingPeriodInput {
                weekday: 5,
                start_time: time(22, 0),
                end_time: time(6, 0),
                spans_next_day: true,
                unpaid_break_minutes: 30,
            },
        ]));

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_periods_overlapping_across_midnight() {
        let result = validate_schedule(&schedule(vec![
            WorkingPeriodInput {
                weekday: 1,
                start_time: time(22, 0),
                end_time: time(6, 0),
                spans_next_day: true,
                unpaid_break_minutes: 0,
            },
            WorkingPeriodInput {
                weekday: 2,
                start_time: time(5, 0),
                end_time: time(8, 0),
                spans_next_day: false,
                unpaid_break_minutes: 0,
            },
        ]));

        assert!(matches!(result, Err(HrError::InvalidInput(_))));
    }

    #[test]
    fn rejects_a_break_as_long_as_the_period() {
        let result = validate_schedule(&schedule(vec![WorkingPeriodInput {
            weekday: 1,
            start_time: time(8, 0),
            end_time: time(9, 0),
            spans_next_day: false,
            unpaid_break_minutes: 60,
        }]));

        assert!(matches!(result, Err(HrError::InvalidInput(_))));
    }
}
