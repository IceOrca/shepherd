use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::super::core::{ShiftAssignmentStatus, StaffingError};

#[derive(Clone, Debug, Serialize, TS)]
pub struct ShiftWorkSession {
    pub id: Uuid,
    pub assignment_id: Uuid,
    pub employee_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub worked_seconds: Option<i64>,
    pub started_latitude: Option<f64>,
    pub started_longitude: Option<f64>,
    pub started_accuracy_meters: Option<f32>,
    pub ended_latitude: Option<f64>,
    pub ended_longitude: Option<f64>,
    pub ended_accuracy_meters: Option<f32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct OwnStaffingAssignment {
    pub assignment_id: Uuid,
    pub shift_id: Uuid,
    pub customer_name: String,
    pub customer_facility_name: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub status: ShiftAssignmentStatus,
    pub observed_worked_seconds: i64,
    pub is_working: bool,
}

#[derive(Clone, Debug)]
pub struct ShiftWorkActionInput {
    pub idempotency_key: Uuid,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub accuracy_meters: Option<f32>,
}

impl ShiftWorkActionInput {
    fn validate(&self) -> Result<(), StaffingError> {
        match (self.latitude, self.longitude) {
            (None, None) if self.accuracy_meters.is_none() => Ok(()),
            (Some(latitude), Some(longitude))
                if (-90.0..=90.0).contains(&latitude)
                    && (-180.0..=180.0).contains(&longitude)
                    && self
                        .accuracy_meters
                        .is_none_or(|accuracy| accuracy.is_finite() && accuracy >= 0.0) =>
            {
                Ok(())
            }
            _ => Err(StaffingError::InvalidInput("work-session location is invalid")),
        }
    }
}

#[async_trait]
pub trait StaffingWorkRepo {
    async fn list_own_assignments(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
    ) -> Result<Vec<OwnStaffingAssignment>, StaffingError>;

    async fn start(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        account_id: Uuid,
        session_id: Uuid,
        input: &ShiftWorkActionInput,
    ) -> Result<ShiftWorkSession, StaffingError>;

    async fn end(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        account_id: Uuid,
        input: &ShiftWorkActionInput,
    ) -> Result<ShiftWorkSession, StaffingError>;
}

pub type DynStaffingWorkRepo = Arc<dyn StaffingWorkRepo + Send + Sync>;

pub struct StaffingWorkService {
    repo: DynStaffingWorkRepo,
}

impl StaffingWorkService {
    pub fn new_arc(repo: DynStaffingWorkRepo) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_own_assignments(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
    ) -> Result<Vec<OwnStaffingAssignment>, StaffingError> {
        self.repo.list_own_assignments(tenant_id, account_id).await
    }

    pub async fn start(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        account_id: Uuid,
        input: ShiftWorkActionInput,
    ) -> Result<ShiftWorkSession, StaffingError> {
        input.validate()?;
        self.repo
            .start(tenant_id, assignment_id, account_id, Uuid::new_v4(), &input)
            .await
    }

    pub async fn end(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        account_id: Uuid,
        input: ShiftWorkActionInput,
    ) -> Result<ShiftWorkSession, StaffingError> {
        input.validate()?;
        self.repo.end(tenant_id, assignment_id, account_id, &input).await
    }
}

#[cfg(test)]
mod tests {
    use super::ShiftWorkActionInput;

    fn input(latitude: Option<f64>, longitude: Option<f64>, accuracy_meters: Option<f32>) -> ShiftWorkActionInput {
        ShiftWorkActionInput {
            idempotency_key: uuid::Uuid::new_v4(),
            latitude,
            longitude,
            accuracy_meters,
        }
    }

    #[test]
    fn accepts_absent_location_and_coordinate_boundaries() {
        assert!(input(None, None, None).validate().is_ok());
        assert!(input(Some(-90.0), Some(-180.0), Some(0.0)).validate().is_ok());
        assert!(input(Some(90.0), Some(180.0), None).validate().is_ok());
    }

    #[test]
    fn rejects_incomplete_location_data() {
        assert!(input(Some(10.7769), None, None).validate().is_err());
        assert!(input(None, Some(106.7009), None).validate().is_err());
        assert!(input(None, None, Some(12.0)).validate().is_err());
    }

    #[test]
    fn rejects_out_of_range_or_non_finite_coordinates() {
        assert!(input(Some(90.1), Some(106.7009), None).validate().is_err());
        assert!(input(Some(10.7769), Some(-180.1), None).validate().is_err());
        assert!(input(Some(f64::NAN), Some(106.7009), None).validate().is_err());
        assert!(input(Some(10.7769), Some(f64::INFINITY), None).validate().is_err());
    }

    #[test]
    fn rejects_negative_or_non_finite_accuracy() {
        assert!(input(Some(10.7769), Some(106.7009), Some(-0.1)).validate().is_err());
        assert!(input(Some(10.7769), Some(106.7009), Some(f32::NAN)).validate().is_err());
        assert!(
            input(Some(10.7769), Some(106.7009), Some(f32::INFINITY))
                .validate()
                .is_err()
        );
    }
}
