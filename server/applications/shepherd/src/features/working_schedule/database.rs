use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use tracing::{error, warn, info, debug, trace};
use crate::features::{
    people::core::{HrError, HrRecordStatus},
    working_schedule::core::{
        EmployeeScheduleAssignment, EmployeeScheduleAssignmentInput, WorkingPeriod, WorkingPeriodInput,
        WorkingSchedule, WorkingScheduleInput, WorkingScheduleRepo,
    },
};
use uuid::Uuid;

use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use sqlx::PgConnection;
pub struct WorkingScheduleProvider {
    db: Arc<DatabaseAdapter>,
}

impl WorkingScheduleProvider {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    async fn begin_active_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, HrError> {
        self.db.begin_tenant(tenant_id).await.map_err(|error| {
            error!(
                "Working schedule tenant transaction failed: tenant_id={} error={}",
                tenant_id, error
            );
            HrError::BackendUnavailable
        })
    }
}

#[derive(Debug)]
struct ScheduleRow {
    id: Uuid,
    code: String,
    name: String,
    time_zone: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug)]
struct PeriodRow {
    id: Uuid,
    schedule_id: Uuid,
    weekday: i16,
    start_time: NaiveTime,
    end_time: NaiveTime,
    spans_next_day: bool,
    unpaid_break_minutes: i16,
}

#[derive(Debug)]
struct ScheduleAssignmentRow {
    id: Uuid,
    employee_id: Uuid,
    schedule_id: Uuid,
    date_start: NaiveDate,
    date_end: Option<NaiveDate>,
    created_at: DateTime<Utc>,
}

#[derive(Debug)]
struct EmploymentDatesRow {
    hire_date: NaiveDate,
    termination_date: Option<NaiveDate>,
}

impl From<PeriodRow> for WorkingPeriod {
    fn from(row: PeriodRow) -> Self {
        Self {
            id: row.id,
            weekday: row.weekday,
            start_time: row.start_time,
            end_time: row.end_time,
            spans_next_day: row.spans_next_day,
            unpaid_break_minutes: row.unpaid_break_minutes,
        }
    }
}

impl From<ScheduleAssignmentRow> for EmployeeScheduleAssignment {
    fn from(row: ScheduleAssignmentRow) -> Self {
        Self {
            id: row.id,
            employee_id: row.employee_id,
            schedule_id: row.schedule_id,
            date_start: row.date_start,
            date_end: row.date_end,
            created_at: row.created_at,
        }
    }
}

#[async_trait]
impl WorkingScheduleRepo for WorkingScheduleProvider {
    async fn list_schedules(&self, tenant_id: Uuid) -> Result<Vec<WorkingSchedule>, HrError> {
        let (schedule_rows, period_rows): (Vec<ScheduleRow>, Vec<PeriodRow>) = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                let schedule_rows: Vec<ScheduleRow> = sqlx::query_as!(
                    ScheduleRow,
                    r#"
                    SELECT id, code, name, time_zone, status, created_at, updated_at
                    FROM hr_working_schedules
                    WHERE tenant_id = $1
                    ORDER BY lower(name), code
                    "#,
                    tenant_id,
                )
                .fetch_all(&mut *connection)
                .await?;
                let period_rows: Vec<PeriodRow> = sqlx::query_as!(
                    PeriodRow,
                    r#"
                    SELECT id, schedule_id, weekday, start_time, end_time, spans_next_day, unpaid_break_minutes
                    FROM hr_working_schedule_periods
                    WHERE tenant_id = $1
                    ORDER BY schedule_id, weekday, start_time
                    "#,
                    tenant_id,
                )
                .fetch_all(connection)
                .await?;
                Ok((schedule_rows, period_rows))
            })
            .await
            .map_err(|error: TenantDbErr| {
                tenant_database_failure("list working schedules and periods", tenant_id, error)
            })?;

        let schedules: Vec<WorkingSchedule> = assemble_schedules(schedule_rows, period_rows)?;
        info!(
            "Tenant working schedules loaded: tenant_id={} schedules={}",
            tenant_id,
            schedules.len()
        );
        Ok(schedules)
    }

    async fn find_schedule(&self, tenant_id: Uuid, schedule_id: Uuid) -> Result<Option<WorkingSchedule>, HrError> {
        let result: Option<(ScheduleRow, Vec<PeriodRow>)> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                let schedule_row: Option<ScheduleRow> = sqlx::query_as!(
                    ScheduleRow,
                    r#"
                    SELECT id, code, name, time_zone, status, created_at, updated_at
                    FROM hr_working_schedules
                    WHERE tenant_id = $1 AND id = $2
                    "#,
                    tenant_id,
                    schedule_id,
                )
                .fetch_optional(&mut *connection)
                .await?;
                let Some(schedule_row) = schedule_row else {
                    return Ok(None);
                };
                let period_rows: Vec<PeriodRow> = sqlx::query_as!(
                    PeriodRow,
                    r#"
                    SELECT id, schedule_id, weekday, start_time, end_time, spans_next_day, unpaid_break_minutes
                    FROM hr_working_schedule_periods
                    WHERE tenant_id = $1 AND schedule_id = $2
                    ORDER BY weekday, start_time
                    "#,
                    tenant_id,
                    schedule_id,
                )
                .fetch_all(connection)
                .await?;
                Ok(Some((schedule_row, period_rows)))
            })
            .await
            .map_err(|error: TenantDbErr| tenant_database_failure("find working schedule", tenant_id, error))?;
        let Some((schedule_row, period_rows)) = result else {
            return Ok(None);
        };
        Ok(Some(assemble_schedule(schedule_row, period_rows)?))
    }

    async fn create_schedule(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        input: &WorkingScheduleInput,
        audit_account_id: Uuid,
    ) -> Result<WorkingSchedule, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        validate_time_zone(&mut transaction, tenant_id, &input.time_zone).await?;
        let schedule_row: ScheduleRow = sqlx::query_as!(
            ScheduleRow,
            r#"
            INSERT INTO hr_working_schedules (
                id, tenant_id, code, name, time_zone, status, created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
            RETURNING id, code, name, time_zone, status, created_at, updated_at
            "#,
            schedule_id,
            tenant_id,
            input.code,
            input.name,
            input.time_zone,
            input.status.as_code(),
            audit_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create working schedule", tenant_id, error))?;
        let period_rows: Vec<PeriodRow> =
            replace_periods(&mut transaction, tenant_id, schedule_id, &input.periods).await?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit working schedule creation", tenant_id, error))?;
        info!(
            "Working schedule created: tenant_id={} schedule_id={} code={} periods={} time_zone={} audit_account_id={}",
            tenant_id,
            schedule_id,
            input.code,
            input.periods.len(),
            input.time_zone,
            audit_account_id
        );
        assemble_schedule(schedule_row, period_rows)
    }

    async fn update_schedule(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        input: &WorkingScheduleInput,
        audit_account_id: Uuid,
    ) -> Result<WorkingSchedule, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        validate_time_zone(&mut transaction, tenant_id, &input.time_zone).await?;
        let current_schedule: Option<ScheduleRow> = sqlx::query_as!(
            ScheduleRow,
            r#"
            SELECT id, code, name, time_zone, status, created_at, updated_at
            FROM hr_working_schedules
            WHERE tenant_id = $1 AND id = $2
            FOR UPDATE
            "#,
            tenant_id,
            schedule_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| database_failure("lock working schedule for update", tenant_id, error))?;
        let current_schedule: ScheduleRow = current_schedule.ok_or(HrError::NotFound)?;
        let current_periods: Vec<PeriodRow> = sqlx::query_as!(
            PeriodRow,
            r#"
            SELECT id, schedule_id, weekday, start_time, end_time, spans_next_day, unpaid_break_minutes
            FROM hr_working_schedule_periods
            WHERE tenant_id = $1 AND schedule_id = $2
            ORDER BY weekday, start_time
            "#,
            tenant_id,
            schedule_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("load working schedule periods for update", tenant_id, error))?;
        let has_assignments: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM hr_employee_schedule_assignments
                WHERE tenant_id = $1 AND schedule_id = $2
            ) AS "exists!"
            "#,
            tenant_id,
            schedule_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("check working schedule assignment history", tenant_id, error))?;
        if has_assignments
            && (current_schedule.time_zone != input.time_zone || !periods_match(&current_periods, &input.periods))
        {
            info!(
                "Working schedule structural update rejected to preserve assignment history: tenant_id={} schedule_id={}",
                tenant_id, schedule_id
            );
            return Err(HrError::Conflict);
        }
        let schedule_row: Option<ScheduleRow> = sqlx::query_as!(
            ScheduleRow,
            r#"
            UPDATE hr_working_schedules
            SET code = $3,
                name = $4,
                time_zone = $5,
                status = $6,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $7
            WHERE tenant_id = $1 AND id = $2
            RETURNING id, code, name, time_zone, status, created_at, updated_at
            "#,
            tenant_id,
            schedule_id,
            input.code,
            input.name,
            input.time_zone,
            input.status.as_code(),
            audit_account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| mutation_failure("update working schedule", tenant_id, error))?;
        let schedule_row: ScheduleRow = schedule_row.ok_or(HrError::NotFound)?;
        let period_rows: Vec<PeriodRow> =
            replace_periods(&mut transaction, tenant_id, schedule_id, &input.periods).await?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit working schedule update", tenant_id, error))?;
        info!(
            "Working schedule updated: tenant_id={} schedule_id={} code={} status={} periods={} audit_account_id={}",
            tenant_id,
            schedule_id,
            input.code,
            input.status.as_code(),
            input.periods.len(),
            audit_account_id
        );
        assemble_schedule(schedule_row, period_rows)
    }

    async fn list_employee_assignments(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<EmployeeScheduleAssignment>, HrError> {
        let result: (bool, Vec<ScheduleAssignmentRow>) = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                let employee_exists: bool = sqlx::query_scalar!(
                    r#"SELECT EXISTS (
                        SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2
                    ) AS "exists!""#,
                    tenant_id,
                    employee_id,
                )
                .fetch_one(&mut *connection)
                .await?;
                let rows: Vec<ScheduleAssignmentRow> = sqlx::query_as!(
                    ScheduleAssignmentRow,
                    r#"
                    SELECT id, employee_id, schedule_id, date_start, date_end, created_at
                    FROM hr_employee_schedule_assignments
                    WHERE tenant_id = $1 AND employee_id = $2
                    ORDER BY date_start DESC, created_at DESC
                    "#,
                    tenant_id,
                    employee_id,
                )
                .fetch_all(connection)
                .await?;
                Ok((employee_exists, rows))
            })
            .await
            .map_err(|error: TenantDbErr| {
                tenant_database_failure("list employee schedule assignments", tenant_id, error)
            })?;
        let (employee_exists, rows): (bool, Vec<ScheduleAssignmentRow>) = result;
        if !employee_exists {
            return Err(HrError::NotFound);
        }
        Ok(rows.into_iter().map(EmployeeScheduleAssignment::from).collect())
    }

    async fn create_employee_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeScheduleAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeScheduleAssignment, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        sqlx::query!(
            r#"SELECT pg_advisory_xact_lock(hashtextextended(($1::UUID)::TEXT || ':' || ($2::UUID)::TEXT, 0))"#,
            tenant_id,
            employee_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error| database_failure("lock employee schedule timeline", tenant_id, error))?;

        let employment_dates: Option<EmploymentDatesRow> = sqlx::query_as!(
            EmploymentDatesRow,
            r#"
            SELECT hire_date, termination_date
            FROM hr_employees
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            employee_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| database_failure("validate employee for schedule assignment", tenant_id, error))?;
        let employment_dates: EmploymentDatesRow = employment_dates.ok_or(HrError::NotFound)?;
        if input.date_start < employment_dates.hire_date
            || employment_dates.termination_date.is_some_and(|termination_date| {
                input.date_start > termination_date
                    || input
                        .date_end
                        .is_none_or(|assignment_end| assignment_end > termination_date)
            })
        {
            return Err(HrError::InvalidInput(
                "working schedule assignment falls outside the employee employment dates",
            ));
        }
        let schedule_status: Option<String> = sqlx::query_scalar!(
            r#"
            SELECT status
            FROM hr_working_schedules
            WHERE tenant_id = $1 AND id = $2
            FOR SHARE
            "#,
            tenant_id,
            input.schedule_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| database_failure("validate assigned working schedule", tenant_id, error))?;
        if schedule_status.as_deref() != Some("active") {
            return Err(HrError::InvalidInput("assigned working schedule is not active"));
        }

        sqlx::query!(
            r#"
            UPDATE hr_employee_schedule_assignments
            SET date_end = $3::date - 1
            WHERE tenant_id = $1
              AND employee_id = $2
              AND date_end IS NULL
              AND date_start < $3
            "#,
            tenant_id,
            employee_id,
            input.date_start,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error| database_failure("close previous working schedule assignment", tenant_id, error))?;

        let overlaps: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM hr_employee_schedule_assignments
                WHERE tenant_id = $1
                  AND employee_id = $2
                  AND daterange(date_start, COALESCE(date_end, 'infinity'::date), '[]')
                      && daterange($3, COALESCE($4, 'infinity'::date), '[]')
            ) AS "exists!"
            "#,
            tenant_id,
            employee_id,
            input.date_start,
            input.date_end,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("check working schedule assignment overlap", tenant_id, error))?;
        if overlaps {
            info!(
                "Working schedule assignment rejected because dates overlap: tenant_id={} employee_id={} schedule_id={} date_start={} date_end={:?}",
                tenant_id, employee_id, input.schedule_id, input.date_start, input.date_end
            );
            return Err(HrError::Conflict);
        }

        let row: ScheduleAssignmentRow = sqlx::query_as!(
            ScheduleAssignmentRow,
            r#"
            INSERT INTO hr_employee_schedule_assignments (
                id, tenant_id, employee_id, schedule_id, date_start, date_end, created_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, employee_id, schedule_id, date_start, date_end, created_at
            "#,
            assignment_id,
            tenant_id,
            employee_id,
            input.schedule_id,
            input.date_start,
            input.date_end,
            audit_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create employee schedule assignment", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit employee schedule assignment", tenant_id, error))?;
        info!(
            "Employee working schedule assigned: tenant_id={} employee_id={} assignment_id={} schedule_id={} date_start={} date_end={:?} audit_account_id={}",
            tenant_id,
            employee_id,
            assignment_id,
            input.schedule_id,
            input.date_start,
            input.date_end,
            audit_account_id
        );
        Ok(row.into())
    }
}

async fn validate_time_zone(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    time_zone: &str,
) -> Result<(), HrError> {
    let exists: bool = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1) AS "exists!""#,
        time_zone,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(|error| database_failure("validate working schedule time zone", tenant_id, error))?;
    if exists {
        Ok(())
    } else {
        Err(HrError::InvalidInput("time zone is not recognized by PostgreSQL"))
    }
}

async fn replace_periods(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    schedule_id: Uuid,
    periods: &[WorkingPeriodInput],
) -> Result<Vec<PeriodRow>, HrError> {
    sqlx::query!(
        "DELETE FROM hr_working_schedule_periods WHERE tenant_id = $1 AND schedule_id = $2",
        tenant_id,
        schedule_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| database_failure("replace working schedule periods", tenant_id, error))?;

    let mut rows: Vec<PeriodRow> = Vec::with_capacity(periods.len());
    for period in periods {
        let row: PeriodRow = sqlx::query_as!(
            PeriodRow,
            r#"
            INSERT INTO hr_working_schedule_periods (
                id, tenant_id, schedule_id, weekday, start_time, end_time, spans_next_day, unpaid_break_minutes
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, schedule_id, weekday, start_time, end_time, spans_next_day, unpaid_break_minutes
            "#,
            Uuid::new_v4(),
            tenant_id,
            schedule_id,
            period.weekday,
            period.start_time,
            period.end_time,
            period.spans_next_day,
            period.unpaid_break_minutes,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("insert working schedule period", tenant_id, error))?;
        rows.push(row);
    }
    Ok(rows)
}

fn assemble_schedules(
    schedule_rows: Vec<ScheduleRow>,
    period_rows: Vec<PeriodRow>,
) -> Result<Vec<WorkingSchedule>, HrError> {
    let mut periods_by_schedule: HashMap<Uuid, Vec<PeriodRow>> = HashMap::new();
    for period in period_rows {
        periods_by_schedule.entry(period.schedule_id).or_default().push(period);
    }
    schedule_rows
        .into_iter()
        .map(|row: ScheduleRow| {
            let periods: Vec<PeriodRow> = periods_by_schedule.remove(&row.id).unwrap_or_default();
            assemble_schedule(row, periods)
        })
        .collect()
}

fn assemble_schedule(row: ScheduleRow, periods: Vec<PeriodRow>) -> Result<WorkingSchedule, HrError> {
    Ok(WorkingSchedule {
        id: row.id,
        code: row.code,
        name: row.name,
        time_zone: row.time_zone,
        status: HrRecordStatus::from_code(&row.status).ok_or(HrError::BackendUnavailable)?,
        periods: periods.into_iter().map(WorkingPeriod::from).collect(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn periods_match(existing: &[PeriodRow], requested: &[WorkingPeriodInput]) -> bool {
    if existing.len() != requested.len() {
        return false;
    }
    let mut existing_values: Vec<(i16, NaiveTime, NaiveTime, bool, i16)> = existing
        .iter()
        .map(|period: &PeriodRow| {
            (
                period.weekday,
                period.start_time,
                period.end_time,
                period.spans_next_day,
                period.unpaid_break_minutes,
            )
        })
        .collect();
    let mut requested_values: Vec<(i16, NaiveTime, NaiveTime, bool, i16)> = requested
        .iter()
        .map(|period: &WorkingPeriodInput| {
            (
                period.weekday,
                period.start_time,
                period.end_time,
                period.spans_next_day,
                period.unpaid_break_minutes,
            )
        })
        .collect();
    existing_values.sort_unstable();
    requested_values.sort_unstable();
    existing_values == requested_values
}

fn database_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> HrError {
    error!(
        "Working schedule db operation failed: operation={} tenant_id={} error={}",
        operation, tenant_id, error
    );
    HrError::BackendUnavailable
}

fn tenant_database_failure(operation: &str, tenant_id: Uuid, error: TenantDbErr) -> HrError {
    error!(
        operation,
        tenant_id = %tenant_id,
        reason = %error,
        "Working schedule automatic tenant operation failed"
    );
    HrError::BackendUnavailable
}

fn mutation_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> HrError {
    let mapped: HrError = error
        .as_database_error()
        .map_or(HrError::BackendUnavailable, |database_error| {
            if database_error.is_unique_violation() {
                HrError::Conflict
            } else if database_error.is_foreign_key_violation() || database_error.is_check_violation() {
                HrError::InvalidInput("a referenced working schedule record is invalid")
            } else {
                HrError::BackendUnavailable
            }
        });
    error!(
        "Working schedule mutation failed: operation={} tenant_id={} mapped_error={:?} error={}",
        operation, tenant_id, mapped, error
    );
    mapped
}
