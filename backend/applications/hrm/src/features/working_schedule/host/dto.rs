use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use crate::features::{
    people::core::HrRecordStatus,
    working_schedule::core::{
        EmployeeScheduleAssignment, EmployeeScheduleAssignmentInput, WorkingPeriodInput, WorkingSchedule,
        WorkingScheduleInput,
    },
};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
pub struct WorkingPeriodRequest {
    #[schema(minimum = 1, maximum = 7)]
    pub weekday: i16,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub spans_next_day: bool,
    pub unpaid_break_minutes: i16,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EmployeeScheduleAssignmentView {
    pub assignment: EmployeeScheduleAssignment,
    pub schedule: WorkingSchedule,
}

impl From<WorkingPeriodRequest> for WorkingPeriodInput {
    fn from(value: WorkingPeriodRequest) -> Self {
        Self {
            weekday: value.weekday,
            start_time: value.start_time,
            end_time: value.end_time,
            spans_next_day: value.spans_next_day,
            unpaid_break_minutes: value.unpaid_break_minutes,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct WorkingScheduleUpsertRequest {
    pub code: String,
    pub name: String,
    pub time_zone: String,
    pub status: HrRecordStatus,
    pub periods: Vec<WorkingPeriodRequest>,
}

impl From<WorkingScheduleUpsertRequest> for WorkingScheduleInput {
    fn from(value: WorkingScheduleUpsertRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            time_zone: value.time_zone.trim().to_owned(),
            status: value.status,
            periods: value.periods.into_iter().map(WorkingPeriodInput::from).collect(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EmployeeScheduleAssignmentCreateRequest {
    pub schedule_id: Uuid,
    pub date_start: NaiveDate,
    pub date_end: Option<NaiveDate>,
}

impl From<EmployeeScheduleAssignmentCreateRequest> for EmployeeScheduleAssignmentInput {
    fn from(value: EmployeeScheduleAssignmentCreateRequest) -> Self {
        Self {
            schedule_id: value.schedule_id,
            date_start: value.date_start,
            date_end: value.date_end,
        }
    }
}
