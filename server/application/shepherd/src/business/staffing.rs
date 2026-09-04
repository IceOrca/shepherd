pub mod core;
pub mod database;
pub mod host;
pub mod planned_work;
pub mod urgent_work;

use std::sync::Arc;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use axum::{
    Router,
    routing::{get, post, put},
};
use tracing::{debug, error, info, trace, warn};
use crate::{AppContext, branch};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileStatus {
    PendingStaff,
    PendingCustomer,
    Matched,
    Discrepancy,
    Reconciled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileCollection {
    Pending,
    Confirmed,
}

#[derive(Debug, Deserialize, TS)]
pub struct ManualRateOverrideRequest {
    pub reason: String,
    pub currency: String,
    pub bill_hourly_rate: String,
    pub worker_hourly_rate: String,
}

impl From<ManualRateOverrideRequest> for ManualRateOverride {
    fn from(value: ManualRateOverrideRequest) -> Self {
        Self {
            reason: value.reason.trim().to_owned(),
            currency: value.currency.trim().to_ascii_uppercase(),
            bill_hourly_rate: value.bill_hourly_rate.trim().to_owned(),
            worker_hourly_rate: value.worker_hourly_rate.trim().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct ReconciliationRevision {
    pub revision_id: Uuid,
    pub assignment_id: Uuid,
    pub revision_number: i32,
    pub worked_seconds: i64,
    pub correction_reason: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ReconciliationCorrectionInput {
    pub expected_revision_id: Uuid,
    pub worked_seconds: i64,
    pub correction_reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StaffingReconcileCursor {
    pub scheduled_starts_at: DateTime<Utc>,
    pub assignment_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct StaffingPriceSetInput {
    pub customer_id: Uuid,
    pub employee_id: Option<Uuid>,
    pub currency: String,
    pub customer_hourly_rate: String,
    pub worker_hourly_rate: String,
    pub effective_from: NaiveDate,
}

#[derive(Clone, Debug)]
pub struct StaffingEligibilityInput {
    pub employee_id: Uuid,
    pub job_id: Uuid,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StaffingShiftInput {
    pub customer_id: Uuid,
    pub job_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub required_workers: i32,
    pub notes: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ManualRateOverride {
    pub reason: String,
    pub currency: String,
    pub bill_hourly_rate: String,
    pub worker_hourly_rate: String,
}

#[derive(Clone, Debug)]
pub struct ShiftAssignmentInput {
    pub employee_id: Uuid,
    pub manual_rate: Option<ManualRateOverride>,
}

#[derive(Clone, Debug)]
pub struct CustomerWorkRecordInput {
    pub confirmed_customer_id: Uuid,
    pub confirmed_started_at: DateTime<Utc>,
    pub confirmed_ended_at: DateTime<Utc>,
    pub customer_reference: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug)]
pub enum StaffingErr {
    NotFound,
    Conflict,
    InvalidInput(&'static str),
    MissingStaffingRate,
    BackendUnavailable,
}
