use std::{collections::BTreeSet, sync::Arc};
use std::ops::{Deref, DerefMut};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use sqlx::postgres::PgQueryResult;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use super::super::{ManualRateOverride, ReconcileCollection, ReconcileStatus};
use super::core::{
    UrgentCustomerCursor, UrgentCustomerPage, UrgentCustomerWorkRecord, UrgentCustomerWorkRecordInput,
    UrgentEmployeeCursor, UrgentEmployeePage, UrgentOwnWorkCursor, UrgentOwnWorkPage, UrgentReconcileCursor,
    UrgentReconcilePage, UrgentTeamWorkPage, UrgentWorkActionSource, UrgentWorkCustomer, UrgentWorkEmployee,
    UrgentWorkEndInput, UrgentStaffingErr, UrgentWorkItem, UrgentWorkManualInput, UrgentWorkReconcile,
    UrgentWorkReconcileInput, UrgentWorkStartInput, UrgentWorkStatus, UrgentWorkSubmissionKind,
};

pub struct UrgentStaffingRepo {
    db: Arc<DatabaseAdapter>,
}

#[derive(Debug)]
struct CustomerRow {
    customer_id: Uuid,
    customer_name: String,
    address: Option<String>,
    time_zone: String,
}

impl From<CustomerRow> for UrgentWorkCustomer {
    fn from(row: CustomerRow) -> Self {
        Self {
            customer_id: row.customer_id,
            customer_name: row.customer_name,
            address: row.address,
            time_zone: row.time_zone,
        }
    }
}

#[derive(Debug)]
struct EmployeeRow {
    employee_id: Uuid,
    employee_code: String,
    display_name: String,
    is_self: bool,
    has_open_work: bool,
}

impl From<EmployeeRow> for UrgentWorkEmployee {
    fn from(row: EmployeeRow) -> Self {
        Self {
            employee_id: row.employee_id,
            employee_code: row.employee_code,
            display_name: row.display_name,
            is_self: row.is_self,
            has_open_work: row.has_open_work,
        }
    }
}

#[derive(Debug)]
struct IdRow {
    id: Uuid,
}

#[derive(Debug)]
struct ExistsRow {
    exists: bool,
}

#[derive(Debug)]
struct WorkDateRow {
    work_date: NaiveDate,
}

#[derive(Debug)]
struct ExistingBatchRow {
    id: Uuid,
    claimed_customer_id: Uuid,
}

#[derive(Debug)]
struct ExistingManualRow {
    report_id: Uuid,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    staff_note: Option<String>,
}

#[derive(Debug)]
struct WorkItemRow {
    report_id: Uuid,
    branch_id: Uuid,
    branch_name: String,
    employee_id: Uuid,
    employee_code: String,
    employee_name: String,
    claimed_customer_id: Uuid,
    customer_name: String,
    submission_kind: String,
    staff_note: Option<String>,
    status: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    worked_seconds: Option<i64>,
    started_by_account_id: Uuid,
    started_by_username: String,
    start_source: String,
    ended_by_account_id: Option<Uuid>,
    ended_by_username: Option<String>,
    end_source: Option<String>,
    reconciled_assignment_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<WorkItemRow> for UrgentWorkItem {
    type Error = UrgentStaffingErr;

    fn try_from(row: WorkItemRow) -> Result<Self, Self::Error> {
        let end_source: Option<UrgentWorkActionSource> = match row.end_source.as_deref() {
            Some(code) => Some(UrgentWorkActionSource::from_code(code).ok_or(UrgentStaffingErr::BackendUnavailable)?),
            None => None,
        };
        Ok(Self {
            report_id: row.report_id,
            branch_id: row.branch_id,
            branch_name: row.branch_name,
            employee_id: row.employee_id,
            employee_code: row.employee_code,
            employee_name: row.employee_name,
            claimed_customer_id: row.claimed_customer_id,
            customer_name: row.customer_name,
            submission_kind: UrgentWorkSubmissionKind::from_code(&row.submission_kind)
                .ok_or(UrgentStaffingErr::BackendUnavailable)?,
            staff_note: row.staff_note,
            status: UrgentWorkStatus::from_code(&row.status).ok_or(UrgentStaffingErr::BackendUnavailable)?,
            started_at: row.started_at,
            ended_at: row.ended_at,
            worked_seconds: row.worked_seconds,
            started_by_account_id: row.started_by_account_id,
            started_by_username: row.started_by_username,
            start_source: UrgentWorkActionSource::from_code(&row.start_source)
                .ok_or(UrgentStaffingErr::BackendUnavailable)?,
            ended_by_account_id: row.ended_by_account_id,
            ended_by_username: row.ended_by_username,
            end_source,
            reconciled_assignment_id: row.reconciled_assignment_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct CustomerRecordRow {
    id: Uuid,
    report_id: Uuid,
    confirmed_customer_id: Uuid,
    confirmed_customer_name: String,
    confirmed_started_at: DateTime<Utc>,
    confirmed_ended_at: DateTime<Utc>,
    confirmed_worked_seconds: i64,
    customer_reference: Option<String>,
    notes: Option<String>,
    updated_at: DateTime<Utc>,
}

impl From<CustomerRecordRow> for UrgentCustomerWorkRecord {
    fn from(row: CustomerRecordRow) -> Self {
        Self {
            id: row.id,
            report_id: row.report_id,
            confirmed_customer_id: row.confirmed_customer_id,
            confirmed_customer_name: row.confirmed_customer_name,
            confirmed_started_at: row.confirmed_started_at,
            confirmed_ended_at: row.confirmed_ended_at,
            confirmed_worked_seconds: row.confirmed_worked_seconds,
            customer_reference: row.customer_reference,
            notes: row.notes,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug)]
struct ReconcileRow {
    report_id: Uuid,
    branch_id: Uuid,
    branch_name: String,
    employee_id: Uuid,
    employee_code: String,
    employee_name: String,
    claimed_customer_id: Uuid,
    customer_name: String,
    submission_kind: String,
    staff_note: Option<String>,
    report_status: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    worked_seconds: Option<i64>,
    started_by_account_id: Uuid,
    started_by_username: String,
    start_source: String,
    ended_by_account_id: Option<Uuid>,
    ended_by_username: Option<String>,
    end_source: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    assignment_id: Option<Uuid>,
    final_customer_id: Option<Uuid>,
    final_job_id: Option<Uuid>,
    final_worked_seconds: Option<i64>,
    adjustment_reason: Option<String>,
    result_revision_id: Option<Uuid>,
    result_revision_number: Option<i32>,
    eligibility_exception_reason: Option<String>,
    customer_record_id: Option<Uuid>,
    confirmed_customer_id: Option<Uuid>,
    confirmed_customer_name: Option<String>,
    confirmed_started_at: Option<DateTime<Utc>>,
    confirmed_ended_at: Option<DateTime<Utc>>,
    confirmed_worked_seconds: Option<i64>,
    customer_reference: Option<String>,
    customer_notes: Option<String>,
    customer_updated_at: Option<DateTime<Utc>>,
}

struct ReconcileRowQuery {
    report_id: Option<Uuid>,
    cursor_active: Option<bool>,
    cursor_started_at: Option<DateTime<Utc>>,
    cursor_report_id: Option<Uuid>,
    customer_id: Option<Uuid>,
    confirmed: Option<bool>,
    period_start: Option<DateTime<Utc>>,
    period_end: Option<DateTime<Utc>>,
    limit: i64,
}

#[derive(Debug)]
struct EndContextRow {
    employee_id: Uuid,
    claimed_customer_id: Uuid,
    report_status: String,
    started_at: DateTime<Utc>,
}

#[derive(Debug)]
struct ReconcileContextRow {
    employee_id: Uuid,
    claimed_customer_id: Uuid,
    report_status: String,
    staff_started_at: DateTime<Utc>,
    staff_ended_at: Option<DateTime<Utc>>,
    staff_worked_seconds: Option<i64>,
    confirmed_customer_id: Option<Uuid>,
    confirmed_started_at: Option<DateTime<Utc>>,
    confirmed_ended_at: Option<DateTime<Utc>>,
    confirmed_worked_seconds: Option<i64>,
    customer_reference: Option<String>,
    customer_notes: Option<String>,
    customer_time_zone: Option<String>,
}

#[derive(Debug)]
struct ResolvedRateRow {
    id: Uuid,
    currency: String,
    hourly_rate: String,
}

impl UrgentStaffingRepo {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    pub async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, UrgentStaffingErr> {
        trace!(operation = "urgent_staffing.begin_tenant", tenant_id = %tenant_id, "Opening urgent-work tenant tran");
        let result: Result<TenantTransaction, TenantDbErr> = self.db.begin_tenant(tenant_id).await;
        result.map_err(|database_error: TenantDbErr| {
            error!(operation = "urgent_staffing.begin_tenant", tenant_id = %tenant_id, reason = %database_error, "Opening urgent-work tenant tran failed");
            UrgentStaffingErr::BackendUnavailable
        })
    }

    pub async fn list_selectable_customers(
        &self,
        tenant_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&UrgentCustomerCursor>,
    ) -> Result<UrgentCustomerPage, UrgentStaffingErr> {
        let search: Option<String> = search.map(str::to_owned);
        let cursor_name: Option<String> = cursor.map(|value: &UrgentCustomerCursor| value.name.clone());
        let cursor_id: Option<Uuid> = cursor.map(|value: &UrgentCustomerCursor| value.customer_id);
        let rows: Vec<CustomerRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerRow,
                    r#"
                    SELECT customer.id AS customer_id, customer.name AS customer_name,
                           customer.address, customer.time_zone
                    FROM business_customers AS customer
                    WHERE customer.tenant_id = $1 AND customer.status = 'active'
                      AND ($2::TEXT IS NULL OR customer.name ILIKE '%' || $2 || '%')
                      AND ($3::TEXT IS NULL OR (lower(customer.name), customer.id) > ($3, $4))
                    ORDER BY lower(customer.name), customer.id
                    LIMIT $5
                    "#,
                    tenant_id,
                    search,
                    cursor_name,
                    cursor_id,
                    limit + 1,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_tran_failure("list urgent customers", tenant_id, err))?;
        let mut items: Vec<UrgentWorkCustomer> = rows.into_iter().map(UrgentWorkCustomer::from).collect();

        let has_more: bool = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor: Option<UrgentCustomerCursor> =
            has_more
                .then(|| items.last())
                .flatten()
                .map(|item| UrgentCustomerCursor {
                    name: item.customer_name.to_lowercase(),
                    customer_id: item.customer_id,
                });
        debug!(tenant_id = %tenant_id, customer_count = items.len(), "Urgent-work customers loaded");
        Ok(UrgentCustomerPage { items, next_cursor })
    }

    pub async fn list_clockable_employees(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&UrgentEmployeeCursor>,
    ) -> Result<UrgentEmployeePage, UrgentStaffingErr> {
        let search: Option<String> = search.map(str::to_owned);
        let cursor_self: Option<bool> = cursor.map(|value| value.is_self);
        let cursor_name: Option<String> = cursor.map(|value| value.name.clone());
        let cursor_id: Option<Uuid> = cursor.map(|value| value.employee_id);
        let rows: Vec<EmployeeRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    EmployeeRow,
                    r#"
                    SELECT employee.id AS employee_id, employee.employee_code,
                           employee.display_name, employee.account_id = $2 AS "is_self!",
                           EXISTS (
                               SELECT 1 FROM business_urgent_work_sessions AS urgent_session
                               WHERE urgent_session.tenant_id = employee.tenant_id
                                 AND urgent_session.employee_id = employee.id
                                 AND urgent_session.ended_at IS NULL
                               UNION ALL
                               SELECT 1 FROM business_shift_work_sessions AS planned_session
                               WHERE planned_session.tenant_id = employee.tenant_id
                                 AND planned_session.employee_id = employee.id
                                 AND planned_session.ended_at IS NULL
                           ) AS "has_open_work!"
                    FROM hr_employees AS employee
                    INNER JOIN accounts AS account
                        ON account.tenant_id = employee.tenant_id
                       AND account.id = employee.account_id
                    WHERE employee.tenant_id = $1
                      AND employee.status = 'active'
                      AND account.status = 'active'
                      AND shepherd_account_has_permission(
                          employee.tenant_id,
                          employee.account_id,
                          employee.branch_id,
                          'business.urgent_work.start'
                      )
                      AND ($3::TEXT IS NULL OR employee.display_name ILIKE '%' || $3 || '%'
                           OR employee.employee_code ILIKE '%' || $3 || '%')
                      AND ($4::BOOLEAN IS NULL
                           OR (employee.account_id = $2) < $4
                           OR ((employee.account_id = $2) = $4
                               AND (lower(employee.display_name), employee.id) > ($5, $6)))
                    ORDER BY (employee.account_id = $2) DESC, lower(employee.display_name), employee.id
                    LIMIT $7
                    "#,
                    tenant_id,
                    actor_account_id,
                    search,
                    cursor_self,
                    cursor_name,
                    cursor_id,
                    limit + 1,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_tran_failure("list urgent employees", tenant_id, err))?;

        let mut items: Vec<UrgentWorkEmployee> = rows.into_iter().map(UrgentWorkEmployee::from).collect();
        let has_more: bool = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor: Option<UrgentEmployeeCursor> =
            has_more
                .then(|| items.last())
                .flatten()
                .map(|item: &UrgentWorkEmployee| UrgentEmployeeCursor {
                    is_self: item.is_self,
                    name: item.display_name.to_lowercase(),
                    employee_id: item.employee_id,
                });
        Ok(UrgentEmployeePage { items, next_cursor })
    }

    pub async fn list_own_work(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        limit: i64,
        cursor: Option<&UrgentOwnWorkCursor>,
    ) -> Result<UrgentOwnWorkPage, UrgentStaffingErr> {
        let cursor_active: Option<bool> = cursor.map(|value: &UrgentOwnWorkCursor| value.active);
        let cursor_started_at: Option<DateTime<Utc>> = cursor.map(|value: &UrgentOwnWorkCursor| value.started_at);
        let cursor_report_id: Option<Uuid> = cursor.map(|value: &UrgentOwnWorkCursor| value.report_id);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<WorkItemRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                load_own_work_items(
                    conn,
                    tenant_id,
                    actor_account_id,
                    cursor_active,
                    cursor_started_at,
                    cursor_report_id,
                    query_limit,
                )
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_tran_failure("list own urgent work", tenant_id, err))?;

        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<UrgentOwnWorkCursor> = if has_more {
            rows.last().map(|row: &WorkItemRow| UrgentOwnWorkCursor {
                active: row.status == "active",
                started_at: row.started_at,
                report_id: row.report_id,
            })
        } else {
            None
        };
        let items: Vec<UrgentWorkItem> = rows
            .into_iter()
            .map(UrgentWorkItem::try_from)
            .collect::<Result<Vec<UrgentWorkItem>, UrgentStaffingErr>>()?;
        Ok(UrgentOwnWorkPage { items, next_cursor })
    }

    pub async fn list_team_work(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        limit: i64,
        cursor: Option<&UrgentOwnWorkCursor>,
    ) -> Result<UrgentTeamWorkPage, UrgentStaffingErr> {
        let cursor_active: Option<bool> = cursor.map(|value| value.active);
        let cursor_started_at: Option<DateTime<Utc>> = cursor.map(|value| value.started_at);
        let cursor_report_id: Option<Uuid> = cursor.map(|value| value.report_id);
        let mut rows: Vec<WorkItemRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                load_work_items(
                    conn,
                    tenant_id,
                    actor_account_id,
                    cursor_active,
                    cursor_started_at,
                    cursor_report_id,
                    limit + 1,
                )
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_tran_failure("list team urgent work", tenant_id, err))?;

        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<UrgentOwnWorkCursor> =
            has_more.then(|| rows.last()).flatten().map(|row| UrgentOwnWorkCursor {
                active: row.status == "active",
                started_at: row.started_at,
                report_id: row.report_id,
            });
        let items: Vec<UrgentWorkItem> = rows
            .into_iter()
            .map(UrgentWorkItem::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(UrgentTeamWorkPage { items, next_cursor })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        allow_peer: bool,
        batch_id: Uuid,
        report_ids: &[Uuid],
        session_ids: &[Uuid],
        input: &UrgentWorkStartInput,
    ) -> Result<Vec<UrgentWorkItem>, UrgentStaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        // Locking the actor before reading the idempotency record serializes
        // concurrent deliveries from the same device/account.
        let actor_employee: Option<IdRow> = sqlx::query_as!(
            IdRow,
            r#"
            SELECT employee.id
            FROM hr_employees AS employee
            JOIN accounts AS account
                ON account.tenant_id = employee.tenant_id
                AND account.id = employee.account_id
            WHERE employee.tenant_id = $1
                AND employee.account_id = $2
                AND employee.status = 'active'
                AND account.status = 'active'
            FOR UPDATE OF employee
            FOR SHARE OF account
            "#,
            tenant_id,
            actor_account_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("resolve urgent actor employee", tenant_id, err))?;
        let actor_employee_id: Uuid = actor_employee.ok_or(UrgentStaffingErr::Forbidden)?.id;
        let existing_batch: Option<ExistingBatchRow> = sqlx::query_as!(
            ExistingBatchRow,
            r#"
            SELECT id, claimed_customer_id
            FROM business_urgent_work_batches
            WHERE tenant_id = $1 AND actor_account_id = $2 AND idempotency_key = $3
            "#,
            tenant_id,
            actor_account_id,
            input.idempotency_key,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("find urgent start idempotency batch", tenant_id, err))?;
        if let Some(existing) = existing_batch {
            if existing.claimed_customer_id != input.customer_id {
                return Err(UrgentStaffingErr::Conflict);
            }
            let rows: Vec<WorkItemRow> = load_batch_items(&mut tran, tenant_id, existing.id).await?;
            let existing_employee_ids: BTreeSet<Uuid> = rows.iter().map(|row: &WorkItemRow| row.employee_id).collect();
            let requested_employee_ids: BTreeSet<Uuid> = input.employee_ids.iter().copied().collect();
            if existing_employee_ids != requested_employee_ids {
                return Err(UrgentStaffingErr::Conflict);
            }
            tran.commit()
                .await
                .map_err(|err: sqlx::Error| tenant_failure("commit idempotent urgent start", tenant_id, err))?;
            return rows.into_iter().map(UrgentWorkItem::try_from).collect();
        }

        let customer_id: Option<Uuid> = sqlx::query_scalar!(
            r#"
            SELECT customer.id
            FROM business_customers AS customer
            JOIN branches AS branch
              ON branch.tenant_id = customer.tenant_id
             AND branch.id = customer.branch_id
            WHERE customer.tenant_id = $1
              AND customer.id = $2
              AND customer.status = 'active'
              AND branch.status = 'active'
            FOR SHARE OF customer, branch
            "#,
            tenant_id,
            input.customer_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("validate urgent customer", tenant_id, err))?;
        if customer_id.is_none() {
            return Err(UrgentStaffingErr::NotFound);
        }

        let target_ids: Vec<IdRow> = sqlx::query_as!(
            IdRow,
            r#"
            SELECT employee.id
            FROM hr_employees AS employee
            INNER JOIN accounts AS account
                ON account.tenant_id = employee.tenant_id
               AND account.id = employee.account_id
            WHERE employee.tenant_id = $1
              AND employee.id = ANY($2)
              AND employee.status = 'active'
              AND account.status = 'active'
              AND shepherd_account_has_permission(
                  employee.tenant_id,
                  employee.account_id,
                  employee.branch_id,
                  'business.urgent_work.start'
              )
            ORDER BY employee.id
            FOR UPDATE OF employee
            FOR SHARE OF account
            "#,
            tenant_id,
            input.employee_ids.as_slice(),
        )
        .fetch_all(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("lock urgent target employees", tenant_id, err))?;
        if target_ids.len() != input.employee_ids.len()
            || report_ids.len() != target_ids.len()
            || session_ids.len() != target_ids.len()
        {
            warn!(
                operation = "urgent_staffing.start",
                tenant_id = %tenant_id,
                actor_account_id = %actor_account_id,
                requested_target_count = input.employee_ids.len(),
                eligible_target_count = target_ids.len(),
                "Rejected urgent-work start because one or more targets are inactive or lack staff-clocking authorization"
            );
            return Err(UrgentStaffingErr::InvalidInput(
                "one or more urgent-work employees are unavailable or not authorized for staff clocking",
            ));
        }
        let target_set: BTreeSet<Uuid> = target_ids.iter().map(|row: &IdRow| row.id).collect();
        let includes_actor: bool = target_set.contains(&actor_employee_id);
        let has_peer: bool = target_set
            .iter()
            .any(|employee_id: &Uuid| *employee_id != actor_employee_id);
        if has_peer && !allow_peer {
            return Err(UrgentStaffingErr::Forbidden);
        }
        let actor_open_work: ExistsRow = sqlx::query_as!(
            ExistsRow,
            r#"
            SELECT EXISTS (
                SELECT 1 FROM business_urgent_work_reports AS report
                INNER JOIN business_urgent_work_sessions AS session
                    ON session.tenant_id = report.tenant_id AND session.report_id = report.id
                WHERE report.tenant_id = $1 AND report.employee_id = $2
                  AND report.claimed_customer_id = $3
                  AND report.status = 'active' AND session.ended_at IS NULL
            ) AS "exists!"
            "#,
            tenant_id,
            actor_employee_id,
            input.customer_id,
        )
        .fetch_one(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("validate urgent peer actor customer", tenant_id, err))?;
        if !includes_actor && !actor_open_work.exists {
            return Err(UrgentStaffingErr::InvalidInput(
                "the first urgent-work batch must include the acting employee",
            ));
        }

        let batch_insert: PgQueryResult = sqlx::query!(
            r#"
            INSERT INTO business_urgent_work_batches (
                id, tenant_id, actor_account_id, claimed_customer_id, idempotency_key
            ) VALUES ($1, $2, $3, $4, $5)
            "#,
            batch_id,
            tenant_id,
            actor_account_id,
            input.customer_id,
            input.idempotency_key,
        )
        .execute(tran.connection())
        .await
        .map_err(|err: sqlx::Error| mutation_failure("insert urgent start batch", tenant_id, err))?;
        trace!(tenant_id = %tenant_id, batch_id = %batch_id, rows_affected = batch_insert.rows_affected(), "Urgent-work start batch inserted");

        let targets: std::iter::Zip<
            std::iter::Zip<std::slice::Iter<'_, IdRow>, std::slice::Iter<'_, Uuid>>,
            std::slice::Iter<'_, Uuid>,
        > = target_ids.iter().zip(report_ids.iter()).zip(session_ids.iter());
        for ((target, report_id), session_id) in targets {
            let report_id: Uuid = *report_id;
            let session_id: Uuid = *session_id;
            let source: &str = if target.id == actor_employee_id { "self" } else { "peer" };
            let report_insert: PgQueryResult = sqlx::query!(
                r#"
                INSERT INTO business_urgent_work_reports (
                    id, tenant_id, start_batch_id, employee_id, claimed_customer_id,
                    created_by_account_id
                ) VALUES ($1, $2, $3, $4, $5, $6)
                "#,
                report_id,
                tenant_id,
                batch_id,
                target.id,
                input.customer_id,
                actor_account_id,
            )
            .execute(tran.connection())
            .await
            .map_err(|err: sqlx::Error| mutation_failure("insert urgent work report", tenant_id, err))?;
            let session_insert: PgQueryResult = sqlx::query!(
                r#"
                INSERT INTO business_urgent_work_sessions (
                    id, tenant_id, report_id, employee_id,
                    started_latitude, started_longitude, started_accuracy_meters,
                    started_by_account_id, start_source
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
                session_id,
                tenant_id,
                report_id,
                target.id,
                input.location.latitude,
                input.location.longitude,
                input.location.accuracy_meters,
                actor_account_id,
                source,
            )
            .execute(tran.connection())
            .await
            .map_err(|err: sqlx::Error| mutation_failure("insert urgent work session", tenant_id, err))?;
            trace!(tenant_id = %tenant_id, report_id = %report_id, session_id = %session_id, report_rows = report_insert.rows_affected(), session_rows = session_insert.rows_affected(), source, "Urgent-work evidence inserted");
            enqueue_notification(
                &mut tran,
                tenant_id,
                "staffing.urgent_work_started",
                session_id,
                report_id,
            )
            .await?;
        }

        let rows: Vec<WorkItemRow> = load_batch_items(&mut tran, tenant_id, batch_id).await?;
        tran.commit()
            .await
            .map_err(|err: sqlx::Error| tenant_failure("commit urgent start", tenant_id, err))?;
        info!(tenant_id = %tenant_id, batch_id = %batch_id, report_count = rows.len(), "Urgent-work batch committed");
        rows.into_iter().map(UrgentWorkItem::try_from).collect()
    }

    pub async fn end(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        allow_peer: bool,
        report_id: Uuid,
        input: &UrgentWorkEndInput,
    ) -> Result<UrgentWorkItem, UrgentStaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let context: EndContextRow = sqlx::query_as!(
            EndContextRow,
            r#"
            SELECT report.employee_id, report.claimed_customer_id,
                   report.status AS report_status, session.started_at
            FROM business_urgent_work_reports AS report
            INNER JOIN business_urgent_work_sessions AS session
                ON session.tenant_id = report.tenant_id AND session.report_id = report.id
            WHERE report.tenant_id = $1 AND report.id = $2
            FOR UPDATE OF report, session
            "#,
            tenant_id,
            report_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("lock urgent end context", tenant_id, err))?
        .ok_or(UrgentStaffingErr::NotFound)?;
        let existing: Option<WorkItemRow> =
            load_by_end_key(&mut tran, tenant_id, actor_account_id, input.idempotency_key).await?;
        if let Some(row) = existing {
            if row.report_id != report_id {
                return Err(UrgentStaffingErr::Conflict);
            }
            tran.commit()
                .await
                .map_err(|err: sqlx::Error| tenant_failure("commit idempotent urgent end", tenant_id, err))?;
            return UrgentWorkItem::try_from(row);
        }
        let report_status: UrgentWorkStatus = UrgentWorkStatus::from_code(&context.report_status).ok_or_else(|| {
            error!(
                operation = "end_urgent_work",
                tenant_id = %tenant_id,
                report_id = %report_id,
                report_status = %context.report_status,
                "Urgent-work report has an unsupported lifecycle status"
            );
            UrgentStaffingErr::BackendUnavailable
        })?;
        if report_status != UrgentWorkStatus::Active {
            return Err(UrgentStaffingErr::Conflict);
        }
        let actor_employee: Option<IdRow> = sqlx::query_as!(
            IdRow,
            "SELECT id FROM hr_employees WHERE tenant_id = $1 AND account_id = $2 AND status = 'active'",
            tenant_id,
            actor_account_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("resolve urgent end actor", tenant_id, err))?;
        let actor_employee_id: Uuid = actor_employee.ok_or(UrgentStaffingErr::Forbidden)?.id;
        let is_self: bool = actor_employee_id == context.employee_id;
        if !is_self {
            if !allow_peer {
                return Err(UrgentStaffingErr::Forbidden);
            }
            let actor_shared_customer: ExistsRow = sqlx::query_as!(
                ExistsRow,
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM business_urgent_work_reports AS actor_report
                    INNER JOIN business_urgent_work_sessions AS actor_session
                        ON actor_session.tenant_id = actor_report.tenant_id
                       AND actor_session.report_id = actor_report.id
                    WHERE actor_report.tenant_id = $1
                      AND actor_report.employee_id = $2
                      AND actor_report.claimed_customer_id = $3
                      AND actor_report.status <> 'cancelled'
                      AND actor_session.started_at <= CURRENT_TIMESTAMP
                      AND (actor_session.ended_at IS NULL OR actor_session.ended_at >= $4)
                ) AS "exists!"
                "#,
                tenant_id,
                actor_employee_id,
                context.claimed_customer_id,
                context.started_at,
            )
            .fetch_one(tran.connection())
            .await
            .map_err(|err: sqlx::Error| database_failure("validate urgent end peer customer", tenant_id, err))?;
            if !actor_shared_customer.exists {
                return Err(UrgentStaffingErr::Forbidden);
            }
        }
        let source: &str = if is_self { "self" } else { "peer" };
        let session_update: PgQueryResult = sqlx::query!(
            r#"
            UPDATE business_urgent_work_sessions
            SET ended_at = CURRENT_TIMESTAMP, end_idempotency_key = $4,
                ended_latitude = $5, ended_longitude = $6, ended_accuracy_meters = $7,
                ended_by_account_id = $3, end_source = $8, updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND report_id = $2 AND ended_at IS NULL
            "#,
            tenant_id,
            report_id,
            actor_account_id,
            input.idempotency_key,
            input.location.latitude,
            input.location.longitude,
            input.location.accuracy_meters,
            source,
        )
        .execute(tran.connection())
        .await
        .map_err(|err: sqlx::Error| mutation_failure("end urgent work session", tenant_id, err))?;
        if session_update.rows_affected() != 1 {
            return Err(UrgentStaffingErr::Conflict);
        }
        let report_update: PgQueryResult = sqlx::query!(
            "UPDATE business_urgent_work_reports SET status = 'completed', updated_at = CURRENT_TIMESTAMP WHERE tenant_id = $1 AND id = $2 AND status = 'active'",
            tenant_id,
            report_id,
        )
        .execute(tran.connection())
        .await
        .map_err(|err: sqlx::Error| mutation_failure("complete urgent work report", tenant_id, err))?;
        if report_update.rows_affected() != 1 {
            return Err(UrgentStaffingErr::Conflict);
        }
        let row: WorkItemRow = load_work_item(&mut tran, tenant_id, report_id).await?;
        enqueue_notification(
            &mut tran,
            tenant_id,
            "staffing.urgent_work_ended",
            row.report_id,
            row.report_id,
        )
        .await?;
        tran.commit()
            .await
            .map_err(|err: sqlx::Error| tenant_failure("commit urgent end", tenant_id, err))?;
        info!(tenant_id = %tenant_id, actor_account_id = %actor_account_id, report_id = %report_id, source, "Urgent-work end committed");
        UrgentWorkItem::try_from(row)
    }

    pub async fn submit_manual(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        batch_id: Uuid,
        report_id: Uuid,
        session_id: Uuid,
        input: &UrgentWorkManualInput,
    ) -> Result<UrgentWorkItem, UrgentStaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let actor_employee: Option<IdRow> = sqlx::query_as!(
            IdRow,
            r#"
            SELECT employee.id
            FROM hr_employees AS employee
            INNER JOIN accounts AS account
                ON account.tenant_id = employee.tenant_id AND account.id = employee.account_id
            WHERE employee.tenant_id = $1 AND employee.account_id = $2
              AND employee.status = 'active' AND account.status = 'active'
              AND shepherd_account_has_permission(
                  employee.tenant_id, employee.account_id, employee.branch_id,
                  'business.urgent_work.start'
              )
            FOR UPDATE OF employee
            FOR SHARE OF account
            "#,
            tenant_id,
            actor_account_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("resolve manual urgent-work employee", tenant_id, err))?;
        let employee_id: Uuid = actor_employee.ok_or(UrgentStaffingErr::Forbidden)?.id;

        let existing_batch: Option<ExistingBatchRow> = sqlx::query_as!(
            ExistingBatchRow,
            r#"
            SELECT id, claimed_customer_id
            FROM business_urgent_work_batches
            WHERE tenant_id = $1 AND actor_account_id = $2 AND idempotency_key = $3
            "#,
            tenant_id,
            actor_account_id,
            input.idempotency_key,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("find manual urgent-work idempotency batch", tenant_id, err))?;
        if let Some(existing_batch) = existing_batch {
            if existing_batch.claimed_customer_id != input.customer_id {
                return Err(UrgentStaffingErr::Conflict);
            }
            let existing: Option<ExistingManualRow> = sqlx::query_as!(
                ExistingManualRow,
                r#"
                SELECT report.id AS report_id, session.started_at,
                       session.ended_at AS "ended_at!", report.staff_note
                FROM business_urgent_work_reports AS report
                INNER JOIN business_urgent_work_sessions AS session
                    ON session.tenant_id = report.tenant_id AND session.report_id = report.id
                WHERE report.tenant_id = $1 AND report.start_batch_id = $2
                  AND report.employee_id = $3 AND report.submission_kind = 'manual'
                "#,
                tenant_id,
                existing_batch.id,
                employee_id,
            )
            .fetch_optional(tran.connection())
            .await
            .map_err(|err: sqlx::Error| database_failure("load idempotent manual urgent work", tenant_id, err))?;
            let existing: ExistingManualRow = existing.ok_or(UrgentStaffingErr::Conflict)?;
            if existing.started_at != input.started_at
                || existing.ended_at != input.ended_at
                || existing.staff_note != input.note
            {
                return Err(UrgentStaffingErr::Conflict);
            }
            let row: WorkItemRow = load_work_item(&mut tran, tenant_id, existing.report_id).await?;
            tran.commit()
                .await
                .map_err(|err: sqlx::Error| tenant_failure("commit idempotent manual urgent work", tenant_id, err))?;
            return UrgentWorkItem::try_from(row);
        }

        let overlaps_existing_staff_evidence: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM business_shift_work_sessions AS session
                JOIN business_shift_assignments AS assignment
                  ON assignment.tenant_id = session.tenant_id
                 AND assignment.id = session.assignment_id
                WHERE session.tenant_id = $1
                  AND session.employee_id = $2
                  AND assignment.status <> 'cancelled'
                  AND tstzrange(session.started_at, session.ended_at, '[)')
                      && tstzrange($3::TIMESTAMPTZ, $4::TIMESTAMPTZ, '[)')
                UNION ALL
                SELECT 1
                FROM business_urgent_work_sessions AS session
                JOIN business_urgent_work_reports AS report
                  ON report.tenant_id = session.tenant_id
                 AND report.id = session.report_id
                WHERE session.tenant_id = $1
                  AND session.employee_id = $2
                  AND report.status <> 'cancelled'
                  AND tstzrange(session.started_at, session.ended_at, '[)')
                      && tstzrange($3::TIMESTAMPTZ, $4::TIMESTAMPTZ, '[)')
            ) AS "exists!"
            "#,
            tenant_id,
            employee_id,
            input.started_at,
            input.ended_at,
        )
        .fetch_one(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("validate manual urgent-work interval", tenant_id, err))?;
        if overlaps_existing_staff_evidence {
            return Err(UrgentStaffingErr::Conflict);
        }

        let customer_id: Option<Uuid> = sqlx::query_scalar!(
            r#"
            SELECT customer.id
            FROM business_customers AS customer
            JOIN branches AS branch
              ON branch.tenant_id = customer.tenant_id
             AND branch.id = customer.branch_id
            JOIN hr_employees AS employee
              ON employee.tenant_id = customer.tenant_id
             AND employee.branch_id = customer.branch_id
            WHERE customer.tenant_id = $1
              AND customer.id = $2
              AND customer.status = 'active'
              AND branch.status = 'active'
              AND employee.id = $3
            FOR SHARE OF customer, branch
            "#,
            tenant_id,
            input.customer_id,
            employee_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("validate manual urgent-work customer", tenant_id, err))?;
        if customer_id.is_none() {
            return Err(UrgentStaffingErr::NotFound);
        }

        sqlx::query!(
            r#"
            INSERT INTO business_urgent_work_batches (
                id, tenant_id, actor_account_id, claimed_customer_id, idempotency_key
            ) VALUES ($1, $2, $3, $4, $5)
            "#,
            batch_id,
            tenant_id,
            actor_account_id,
            input.customer_id,
            input.idempotency_key,
        )
        .execute(tran.connection())
        .await
        .map_err(|err: sqlx::Error| mutation_failure("insert manual urgent-work batch", tenant_id, err))?;
        sqlx::query!(
            r#"
            INSERT INTO business_urgent_work_reports (
                id, tenant_id, start_batch_id, employee_id, claimed_customer_id,
                status, created_by_account_id, submission_kind, staff_note
            ) VALUES ($1, $2, $3, $4, $5, 'completed', $6, 'manual', $7)
            "#,
            report_id,
            tenant_id,
            batch_id,
            employee_id,
            input.customer_id,
            actor_account_id,
            input.note,
        )
        .execute(tran.connection())
        .await
        .map_err(|err: sqlx::Error| mutation_failure("insert manual urgent-work report", tenant_id, err))?;
        sqlx::query!(
            r#"
            INSERT INTO business_urgent_work_sessions (
                id, tenant_id, report_id, employee_id, started_at, ended_at,
                end_idempotency_key, started_by_account_id, start_source,
                ended_by_account_id, end_source
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'self', $8, 'self')
            "#,
            session_id,
            tenant_id,
            report_id,
            employee_id,
            input.started_at,
            input.ended_at,
            input.idempotency_key,
            actor_account_id,
        )
        .execute(tran.connection())
        .await
        .map_err(|err: sqlx::Error| mutation_failure("insert manual urgent-work session", tenant_id, err))?;
        enqueue_notification(
            &mut tran,
            tenant_id,
            "staffing.urgent_work_manually_declared",
            session_id,
            report_id,
        )
        .await?;
        let row: WorkItemRow = load_work_item(&mut tran, tenant_id, report_id).await?;
        tran.commit()
            .await
            .map_err(|err: sqlx::Error| tenant_failure("commit manual urgent work", tenant_id, err))?;
        info!(tenant_id = %tenant_id, actor_account_id = %actor_account_id, report_id = %report_id, "Manual urgent-work declaration committed");
        UrgentWorkItem::try_from(row)
    }

    pub async fn cancel(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        report_id: Uuid,
        reason: &str,
    ) -> Result<(), UrgentStaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let context = sqlx::query!(
            r#"
            SELECT report.status,
                   session.ended_at,
                   EXISTS (
                       SELECT 1
                       FROM business_shift_assignments AS assignment
                       WHERE assignment.tenant_id = report.tenant_id
                         AND assignment.urgent_work_report_id = report.id
                   ) AS "has_assignment!"
            FROM business_urgent_work_reports AS report
            JOIN business_urgent_work_sessions AS session
              ON session.tenant_id = report.tenant_id
             AND session.report_id = report.id
            WHERE report.tenant_id = $1
              AND report.id = $2
            FOR UPDATE OF report, session
            "#,
            tenant_id,
            report_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("lock urgent work for cancellation", tenant_id, err))?
        .ok_or(UrgentStaffingErr::NotFound)?;
        if context.status != "completed" || context.ended_at.is_none() || context.has_assignment {
            return Err(UrgentStaffingErr::Conflict);
        }

        let updated: PgQueryResult = sqlx::query!(
            r#"
            UPDATE business_urgent_work_reports
            SET status = 'cancelled',
                cancellation_reason = $3,
                cancelled_at = CURRENT_TIMESTAMP,
                cancelled_by_account_id = $4,
                updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1
              AND id = $2
              AND status = 'completed'
            "#,
            tenant_id,
            report_id,
            reason,
            actor_account_id,
        )
        .execute(tran.connection())
        .await
        .map_err(|err| mutation_failure("cancel urgent work report", tenant_id, err))?;
        if updated.rows_affected() != 1 {
            return Err(UrgentStaffingErr::Conflict);
        }
        tran.commit()
            .await
            .map_err(|err| database_failure("commit urgent work cancellation", tenant_id, err))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_reconciliations(
        &self,
        tenant_id: Uuid,
        customer_id: Option<Uuid>,
        collection: ReconcileCollection,
        period_start: Option<DateTime<Utc>>,
        period_end: Option<DateTime<Utc>>,
        limit: i64,
        cursor: Option<&UrgentReconcileCursor>,
    ) -> Result<UrgentReconcilePage, UrgentStaffingErr> {
        let cursor_active: Option<bool> = cursor.map(|value: &UrgentReconcileCursor| value.active);
        let cursor_started_at: Option<DateTime<Utc>> = cursor.map(|value: &UrgentReconcileCursor| value.started_at);
        let cursor_report_id: Option<Uuid> = cursor.map(|value: &UrgentReconcileCursor| value.report_id);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<ReconcileRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                load_reconciliation_rows(
                    conn,
                    tenant_id,
                    ReconcileRowQuery {
                        report_id: None,
                        cursor_active,
                        cursor_started_at,
                        cursor_report_id,
                        customer_id,
                        confirmed: Some(collection == ReconcileCollection::Confirmed),
                        period_start,
                        period_end,
                        limit: query_limit,
                    },
                )
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_tran_failure("list urgent reconciliations", tenant_id, err))?;
        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<UrgentReconcileCursor> = if has_more {
            rows.last().map(|row: &ReconcileRow| UrgentReconcileCursor {
                active: row.report_status == "active",
                started_at: row.started_at,
                report_id: row.report_id,
            })
        } else {
            None
        };
        let items: Vec<UrgentWorkReconcile> = rows
            .into_iter()
            .map(reconciliation_from_row)
            .collect::<Result<Vec<UrgentWorkReconcile>, UrgentStaffingErr>>()?;
        Ok(UrgentReconcilePage { items, next_cursor })
    }

    pub async fn upsert_customer_record(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        record_id: Uuid,
        report_id: Uuid,
        input: &UrgentCustomerWorkRecordInput,
        allow_terminal_correction: bool,
    ) -> Result<UrgentCustomerWorkRecord, UrgentStaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let status: Option<String> = sqlx::query_scalar!(
            "SELECT status FROM business_urgent_work_reports WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
            tenant_id,
            report_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err| database_failure("lock urgent customer evidence report", tenant_id, err))?;
        match status.as_deref() {
            None => return Err(UrgentStaffingErr::NotFound),
            Some("completed") => {}
            Some("reconciled") if allow_terminal_correction => {}
            Some(_) => return Err(UrgentStaffingErr::Conflict),
        }
        if status.as_deref() == Some("reconciled") {
            let dates_open: bool = sqlx::query_scalar!(
                r#"
                SELECT shepherd_financial_date_is_open_for_update(report.tenant_id, report.branch_id,
                           (current_record.confirmed_started_at AT TIME ZONE current_customer.time_zone)::DATE)
                       AND shepherd_financial_date_is_open_for_update(report.tenant_id, report.branch_id,
                           ($3::TIMESTAMPTZ AT TIME ZONE proposed_customer.time_zone)::DATE) AS "dates_open!"
                FROM business_urgent_work_reports AS report
                JOIN business_urgent_customer_work_records AS current_record
                  ON current_record.tenant_id = report.tenant_id AND current_record.report_id = report.id
                JOIN business_customers AS current_customer
                  ON current_customer.tenant_id = current_record.tenant_id AND current_customer.id = current_record.confirmed_customer_id
                JOIN business_customers AS proposed_customer
                  ON proposed_customer.tenant_id = report.tenant_id AND proposed_customer.id = $4
                WHERE report.tenant_id = $1 AND report.id = $2
                "#,
                tenant_id,
                report_id,
                input.confirmed_started_at,
                input.confirmed_customer_id,
            )
            .fetch_one(tran.connection())
            .await
            .map_err(|err| database_failure("validate urgent evidence periods", tenant_id, err))?;
            if !dates_open {
                return Err(UrgentStaffingErr::Conflict);
            }
        }
        let result: PgQueryResult = sqlx::query!(
            r#"
            INSERT INTO business_urgent_customer_work_records (
                id, tenant_id, report_id, confirmed_customer_id,
                confirmed_started_at, confirmed_ended_at, customer_reference,
                notes, recorded_by_account_id
            )
            SELECT $1, $2, $3, customer.id, $5, $6, $7, $8, $9
            FROM business_customers AS customer
            WHERE customer.tenant_id = $2 AND customer.id = $4 AND customer.status = 'active'
            ON CONFLICT (tenant_id, report_id) DO UPDATE
            SET confirmed_customer_id = EXCLUDED.confirmed_customer_id,
                confirmed_started_at = EXCLUDED.confirmed_started_at,
                confirmed_ended_at = EXCLUDED.confirmed_ended_at,
                customer_reference = EXCLUDED.customer_reference,
                notes = EXCLUDED.notes,
                recorded_by_account_id = EXCLUDED.recorded_by_account_id,
                updated_at = CURRENT_TIMESTAMP
            "#,
            record_id,
            tenant_id,
            report_id,
            input.confirmed_customer_id,
            input.confirmed_started_at,
            input.confirmed_ended_at,
            input.customer_reference,
            input.notes,
            actor_account_id,
        )
        .execute(tran.connection())
        .await
        .map_err(|err: sqlx::Error| mutation_failure("upsert urgent customer evidence", tenant_id, err))?;
        if result.rows_affected() != 1 {
            return Err(UrgentStaffingErr::Conflict);
        }
        let row: CustomerRecordRow = load_customer_record(&mut tran, tenant_id, report_id).await?;
        tran.commit()
            .await
            .map_err(|err: sqlx::Error| tenant_failure("commit urgent customer evidence", tenant_id, err))?;
        Ok(row.into())
    }

    pub async fn reconcile(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        shift_id: Uuid,
        assignment_id: Uuid,
        report_id: Uuid,
        input: &UrgentWorkReconcileInput,
    ) -> Result<UrgentWorkReconcile, UrgentStaffingErr> {
        reconcile_report(
            self,
            tenant_id,
            actor_account_id,
            shift_id,
            assignment_id,
            report_id,
            input,
        )
        .await
    }

    pub async fn accept_staff_record(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        shift_id: Uuid,
        assignment_id: Uuid,
        report_id: Uuid,
        job_id: Uuid,
    ) -> Result<UrgentWorkReconcile, UrgentStaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let staff = sqlx::query!(
            r#"
            SELECT report.status, report.claimed_customer_id,
                   session.started_at, session.ended_at, session.worked_seconds,
                   customer_record.confirmed_customer_id AS "confirmed_customer_id?",
                   customer_record.confirmed_started_at AS "confirmed_started_at?",
                   customer_record.confirmed_ended_at AS "confirmed_ended_at?",
                   customer_record.confirmed_worked_seconds AS "confirmed_worked_seconds?"
            FROM business_urgent_work_reports AS report
            INNER JOIN business_urgent_work_sessions AS session
                ON session.tenant_id = report.tenant_id AND session.report_id = report.id
            LEFT JOIN business_urgent_customer_work_records AS customer_record
                ON customer_record.tenant_id = report.tenant_id AND customer_record.report_id = report.id
            WHERE report.tenant_id = $1 AND report.id = $2
            FOR UPDATE OF report, session
            "#,
            tenant_id,
            report_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err| database_failure("lock urgent staff evidence acceptance", tenant_id, err))?
        .ok_or(UrgentStaffingErr::NotFound)?;
        if staff.status != "completed" {
            return Err(UrgentStaffingErr::Conflict);
        }
        let ended_at: DateTime<Utc> = staff.ended_at.ok_or(UrgentStaffingErr::Conflict)?;
        let worked_seconds: i64 = staff
            .worked_seconds
            .filter(|value| *value > 0)
            .ok_or(UrgentStaffingErr::Conflict)?;
        let evidence_matches: bool = staff.confirmed_customer_id == Some(staff.claimed_customer_id)
            && staff.confirmed_started_at == Some(staff.started_at)
            && staff.confirmed_ended_at == Some(ended_at)
            && staff.confirmed_worked_seconds == Some(worked_seconds);
        if !evidence_matches {
            return Err(UrgentStaffingErr::Conflict);
        }
        let input: UrgentWorkReconcileInput = UrgentWorkReconcileInput {
            final_customer_id: staff.claimed_customer_id,
            job_id,
            worked_seconds,
            adjustment_reason: None,
            manual_rate: None,
        };
        let reconciliation: UrgentWorkReconcile = reconcile_report_in_transaction(
            &mut tran,
            tenant_id,
            actor_account_id,
            shift_id,
            assignment_id,
            report_id,
            &input,
        )
        .await?;
        tran.commit()
            .await
            .map_err(|err| tenant_failure("commit accepted urgent staff evidence", tenant_id, err))?;
        info!(tenant_id = %tenant_id, report_id = %report_id, assignment_id = %assignment_id, "Urgent staff evidence accepted atomically");
        Ok(reconciliation)
    }
}

async fn load_own_work_items(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    cursor_active: Option<bool>,
    cursor_started_at: Option<DateTime<Utc>>,
    cursor_report_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<WorkItemRow>, sqlx::Error> {
    sqlx::query_as!(
        WorkItemRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, customer.name AS customer_name,
               report.submission_kind, report.staff_note, report.status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               assignment.id AS "reconciled_assignment_id?",
               report.created_at, report.updated_at
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = report.tenant_id AND customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        WHERE report.tenant_id = $1 AND employee.account_id = $2
          AND ($3::BOOLEAN IS NULL
               OR ((report.status = 'active'), session.started_at, report.id)
                  < ($3, $4::TIMESTAMPTZ, $5::UUID))
        ORDER BY (report.status = 'active') DESC, session.started_at DESC, report.id DESC
        LIMIT $6
        "#,
        tenant_id,
        actor_account_id,
        cursor_active,
        cursor_started_at,
        cursor_report_id,
        limit,
    )
    .fetch_all(conn)
    .await
}

async fn load_work_items(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    cursor_active: Option<bool>,
    cursor_started_at: Option<DateTime<Utc>>,
    cursor_report_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<WorkItemRow>, sqlx::Error> {
    sqlx::query_as!(
        WorkItemRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, customer.name AS customer_name,
               report.submission_kind, report.staff_note, report.status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               assignment.id AS "reconciled_assignment_id?",
               report.created_at, report.updated_at
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = report.tenant_id AND customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        WHERE report.tenant_id = $1
            AND report.claimed_customer_id IN (
                  SELECT actor_report.claimed_customer_id
                  FROM business_urgent_work_reports AS actor_report
                  INNER JOIN hr_employees AS actor_employee
                      ON actor_employee.tenant_id = actor_report.tenant_id
                     AND actor_employee.id = actor_report.employee_id
                  WHERE actor_report.tenant_id = $1 AND actor_employee.account_id = $2
                    AND actor_report.status <> 'cancelled'
                    AND actor_report.created_at >= CURRENT_TIMESTAMP - INTERVAL '24 hours'
            )
            AND ($3::BOOLEAN IS NULL
                OR ((report.status = 'active'), session.started_at, report.id)
                  < ($3, $4::TIMESTAMPTZ, $5::UUID))
        ORDER BY (report.status = 'active') DESC, session.started_at DESC, report.id DESC
        LIMIT $6
        "#,
        tenant_id,
        actor_account_id,
        cursor_active,
        cursor_started_at,
        cursor_report_id,
        limit,
    )
    .fetch_all(conn)
    .await
}

async fn load_batch_items(
    tran: &mut TenantTransaction,
    tenant_id: Uuid,
    batch_id: Uuid,
) -> Result<Vec<WorkItemRow>, UrgentStaffingErr> {
    sqlx::query_as!(
        WorkItemRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, customer.name AS customer_name,
               report.submission_kind, report.staff_note, report.status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               assignment.id AS "reconciled_assignment_id?",
               report.created_at, report.updated_at
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = report.tenant_id AND customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        WHERE report.tenant_id = $1 AND report.start_batch_id = $2
        ORDER BY employee.display_name, report.id
        "#,
        tenant_id,
        batch_id,
    )
    .fetch_all(tran.connection())
    .await
    .map_err(|err: sqlx::Error| database_failure("load urgent batch items", tenant_id, err))
}

async fn load_work_item(
    tran: &mut TenantTransaction,
    tenant_id: Uuid,
    report_id: Uuid,
) -> Result<WorkItemRow, UrgentStaffingErr> {
    sqlx::query_as!(
        WorkItemRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, customer.name AS customer_name,
               report.submission_kind, report.staff_note, report.status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               assignment.id AS "reconciled_assignment_id?",
               report.created_at, report.updated_at
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = report.tenant_id AND customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        WHERE report.tenant_id = $1 AND report.id = $2
        "#,
        tenant_id,
        report_id,
    )
    .fetch_optional(tran.connection())
    .await
    .map_err(|err: sqlx::Error| database_failure("load urgent work item", tenant_id, err))?
    .ok_or(UrgentStaffingErr::NotFound)
}

async fn load_by_end_key(
    tran: &mut TenantTransaction,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    idempotency_key: Uuid,
) -> Result<Option<WorkItemRow>, UrgentStaffingErr> {
    sqlx::query_as!(
        WorkItemRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, customer.name AS customer_name,
               report.submission_kind, report.staff_note, report.status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               assignment.id AS "reconciled_assignment_id?",
               report.created_at, report.updated_at
        FROM business_urgent_work_sessions AS session
        INNER JOIN business_urgent_work_reports AS report
            ON report.tenant_id = session.tenant_id AND report.id = session.report_id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = report.tenant_id AND customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        WHERE session.tenant_id = $1 AND session.ended_by_account_id = $2
          AND session.end_idempotency_key = $3
        "#,
        tenant_id,
        actor_account_id,
        idempotency_key,
    )
    .fetch_optional(tran.connection())
    .await
    .map_err(|err: sqlx::Error| database_failure("load idempotent urgent end", tenant_id, err))
}

async fn load_customer_record(
    tran: &mut TenantTransaction,
    tenant_id: Uuid,
    report_id: Uuid,
) -> Result<CustomerRecordRow, UrgentStaffingErr> {
    sqlx::query_as!(
        CustomerRecordRow,
        r#"
        SELECT record.id, record.report_id, record.confirmed_customer_id,
               customer.name AS confirmed_customer_name,
               record.confirmed_started_at, record.confirmed_ended_at,
               record.confirmed_worked_seconds AS "confirmed_worked_seconds!",
               record.customer_reference, record.notes, record.updated_at
        FROM business_urgent_customer_work_records AS record
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = record.tenant_id AND customer.id = record.confirmed_customer_id
        WHERE record.tenant_id = $1 AND record.report_id = $2
        "#,
        tenant_id,
        report_id,
    )
    .fetch_optional(tran.connection())
    .await
    .map_err(|err: sqlx::Error| database_failure("load urgent customer evidence", tenant_id, err))?
    .ok_or(UrgentStaffingErr::NotFound)
}

async fn load_reconciliation_rows(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    query: ReconcileRowQuery,
) -> Result<Vec<ReconcileRow>, sqlx::Error> {
    sqlx::query_as!(
        ReconcileRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, claimed_customer.name AS customer_name,
               report.submission_kind, report.staff_note,
               report.status AS report_status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               report.created_at, report.updated_at,
               assignment.id AS "assignment_id?",
               final_shift.customer_id AS "final_customer_id?",
               final_shift.job_id AS "final_job_id?",
               result.worked_seconds AS "final_worked_seconds?",
               result.adjustment_reason AS "adjustment_reason?",
               result.revision_id AS "result_revision_id?",
               result.revision_number AS "result_revision_number?",
               assignment.eligibility_exception_reason,
               customer_record.id AS "customer_record_id?",
               customer_record.confirmed_customer_id AS "confirmed_customer_id?",
               confirmed_customer.name AS "confirmed_customer_name?",
               customer_record.confirmed_started_at AS "confirmed_started_at?",
               customer_record.confirmed_ended_at AS "confirmed_ended_at?",
               customer_record.confirmed_worked_seconds AS "confirmed_worked_seconds?",
               customer_record.customer_reference,
               customer_record.notes AS customer_notes,
               customer_record.updated_at AS "customer_updated_at?"
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS claimed_customer
            ON claimed_customer.tenant_id = report.tenant_id
           AND claimed_customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_urgent_customer_work_records AS customer_record
            ON customer_record.tenant_id = report.tenant_id AND customer_record.report_id = report.id
        LEFT JOIN business_customers AS confirmed_customer
            ON confirmed_customer.tenant_id = customer_record.tenant_id
           AND confirmed_customer.id = customer_record.confirmed_customer_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        LEFT JOIN business_staffing_shifts AS final_shift
            ON final_shift.tenant_id = assignment.tenant_id AND final_shift.id = assignment.shift_id
        LEFT JOIN LATERAL (
            SELECT revision_id, revision_number, worked_seconds, adjustment_reason, confirmed_started_at
            FROM business_assignment_reconciliation_revisions
            WHERE tenant_id = assignment.tenant_id AND assignment_id = assignment.id
            ORDER BY revision_number DESC
            LIMIT 1
        ) AS result ON TRUE
        WHERE report.tenant_id = $1 AND ($2::UUID IS NULL OR report.id = $2)
          AND ($3::BOOLEAN IS NULL
               OR ((report.status = 'active'), session.started_at, report.id)
                  < ($3, $4::TIMESTAMPTZ, $5::UUID))
          AND ($6::UUID IS NULL
               OR report.claimed_customer_id = $6
               OR customer_record.confirmed_customer_id = $6
               OR final_shift.customer_id = $6)
          AND ($7::BOOLEAN IS NULL OR ($7 AND report.status = 'reconciled')
               OR (NOT $7 AND report.status <> 'reconciled' AND report.status <> 'cancelled'))
          AND ($7::BOOLEAN IS NULL OR NOT $7 OR (result.confirmed_started_at >= $8
                          AND result.confirmed_started_at < $9))
        ORDER BY (report.status = 'active') DESC, session.started_at DESC, report.id DESC
        LIMIT $10
        "#,
        tenant_id,
        query.report_id,
        query.cursor_active,
        query.cursor_started_at,
        query.cursor_report_id,
        query.customer_id,
        query.confirmed,
        query.period_start,
        query.period_end,
        query.limit,
    )
    .fetch_all(conn)
    .await
}

fn reconciliation_from_row(row: ReconcileRow) -> Result<UrgentWorkReconcile, UrgentStaffingErr> {
    let report_status: UrgentWorkStatus =
        UrgentWorkStatus::from_code(&row.report_status).ok_or(UrgentStaffingErr::BackendUnavailable)?;
    let start_source: UrgentWorkActionSource =
        UrgentWorkActionSource::from_code(&row.start_source).ok_or(UrgentStaffingErr::BackendUnavailable)?;
    let end_source: Option<UrgentWorkActionSource> = match row.end_source.as_deref() {
        Some(code) => Some(UrgentWorkActionSource::from_code(code).ok_or(UrgentStaffingErr::BackendUnavailable)?),
        None => None,
    };
    let customer_record: Option<UrgentCustomerWorkRecord> = match (
        row.customer_record_id,
        row.confirmed_customer_id,
        row.confirmed_customer_name,
        row.confirmed_started_at,
        row.confirmed_ended_at,
        row.confirmed_worked_seconds,
        row.customer_updated_at,
    ) {
        (
            Some(id),
            Some(customer_id),
            Some(customer_name),
            Some(started_at),
            Some(ended_at),
            Some(worked_seconds),
            Some(updated_at),
        ) => Some(UrgentCustomerWorkRecord {
            id,
            report_id: row.report_id,
            confirmed_customer_id: customer_id,
            confirmed_customer_name: customer_name,
            confirmed_started_at: started_at,
            confirmed_ended_at: ended_at,
            confirmed_worked_seconds: worked_seconds,
            customer_reference: row.customer_reference,
            notes: row.customer_notes,
            updated_at,
        }),
        (None, None, None, None, None, None, None) => None,
        _ => return Err(UrgentStaffingErr::BackendUnavailable),
    };
    let staff_worked_seconds: i64 = row.worked_seconds.unwrap_or(0);
    let reconciliation_status: ReconcileStatus = if report_status == UrgentWorkStatus::Reconciled {
        ReconcileStatus::Reconciled
    } else if report_status == UrgentWorkStatus::Active || staff_worked_seconds <= 0 {
        ReconcileStatus::PendingStaff
    } else if customer_record.is_none() {
        ReconcileStatus::PendingCustomer
    } else if customer_record
        .as_ref()
        .is_some_and(|record: &UrgentCustomerWorkRecord| {
            record.confirmed_customer_id == row.claimed_customer_id
                && record.confirmed_started_at == row.started_at
                && row
                    .ended_at
                    .is_some_and(|ended_at: DateTime<Utc>| record.confirmed_ended_at == ended_at)
                && record.confirmed_worked_seconds == staff_worked_seconds
        })
    {
        ReconcileStatus::Matched
    } else {
        ReconcileStatus::Discrepancy
    };
    Ok(UrgentWorkReconcile {
        work: UrgentWorkItem {
            report_id: row.report_id,
            branch_id: row.branch_id,
            branch_name: row.branch_name,
            employee_id: row.employee_id,
            employee_code: row.employee_code,
            employee_name: row.employee_name,
            claimed_customer_id: row.claimed_customer_id,
            customer_name: row.customer_name,
            submission_kind: UrgentWorkSubmissionKind::from_code(&row.submission_kind)
                .ok_or(UrgentStaffingErr::BackendUnavailable)?,
            staff_note: row.staff_note,
            status: report_status,
            started_at: row.started_at,
            ended_at: row.ended_at,
            worked_seconds: row.worked_seconds,
            started_by_account_id: row.started_by_account_id,
            started_by_username: row.started_by_username,
            start_source,
            ended_by_account_id: row.ended_by_account_id,
            ended_by_username: row.ended_by_username,
            end_source,
            reconciled_assignment_id: row.assignment_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        },
        customer_record,
        reconciliation_status,
        final_customer_id: row.final_customer_id,
        final_job_id: row.final_job_id,
        final_worked_seconds: row.final_worked_seconds,
        adjustment_reason: row.adjustment_reason,
        eligibility_exception_reason: row.eligibility_exception_reason,
        result_revision_id: row.result_revision_id,
        result_revision_number: row.result_revision_number,
    })
}

pub async fn reconcile_report(
    provider: &UrgentStaffingRepo,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    shift_id: Uuid,
    assignment_id: Uuid,
    report_id: Uuid,
    input: &UrgentWorkReconcileInput,
) -> Result<UrgentWorkReconcile, UrgentStaffingErr> {
    let mut tran: TenantTransaction = provider.begin_tenant(tenant_id).await?;
    let reconciliation: UrgentWorkReconcile = reconcile_report_in_transaction(
        &mut tran,
        tenant_id,
        actor_account_id,
        shift_id,
        assignment_id,
        report_id,
        input,
    )
    .await?;
    tran.commit()
        .await
        .map_err(|err: sqlx::Error| tenant_failure("commit urgent reconciliation", tenant_id, err))?;
    info!(tenant_id = %tenant_id, report_id = %report_id, assignment_id = %assignment_id, "Urgent reconciliation committed");
    Ok(reconciliation)
}

pub async fn reconcile_report_in_transaction(
    tran: &mut TenantTransaction,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    shift_id: Uuid,
    assignment_id: Uuid,
    report_id: Uuid,
    input: &UrgentWorkReconcileInput,
) -> Result<UrgentWorkReconcile, UrgentStaffingErr> {
    let status: Option<String> = sqlx::query_scalar!(
        "SELECT status FROM business_urgent_work_reports WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        tenant_id,
        report_id,
    )
    .fetch_optional(tran.connection())
    .await
    .map_err(|err: sqlx::Error| database_failure("lock urgent report for reconciliation", tenant_id, err))?;
    match status.as_deref() {
        None => return Err(UrgentStaffingErr::NotFound),
        Some("completed") => {}
        Some(_) => return Err(UrgentStaffingErr::Conflict),
    }
    let context: ReconcileContextRow = sqlx::query_as!(
        ReconcileContextRow,
        r#"
        SELECT report.employee_id, report.claimed_customer_id,
               report.status AS report_status, session.started_at AS staff_started_at,
               session.ended_at AS staff_ended_at, session.worked_seconds AS staff_worked_seconds,
               customer_record.confirmed_customer_id AS "confirmed_customer_id?",
               customer_record.confirmed_started_at AS "confirmed_started_at?",
               customer_record.confirmed_ended_at AS "confirmed_ended_at?",
               customer_record.confirmed_worked_seconds AS "confirmed_worked_seconds?",
               customer_record.customer_reference, customer_record.notes AS customer_notes,
               final_customer.time_zone AS "customer_time_zone?"
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        LEFT JOIN business_urgent_customer_work_records AS customer_record
            ON customer_record.tenant_id = report.tenant_id AND customer_record.report_id = report.id
        LEFT JOIN business_customers AS final_customer
            ON final_customer.tenant_id = report.tenant_id AND final_customer.id = $3
        WHERE report.tenant_id = $1 AND report.id = $2
        FOR UPDATE OF report, session, employee
        "#,
        tenant_id,
        report_id,
        input.final_customer_id,
    )
    .fetch_optional(tran.connection())
    .await
    .map_err(|err: sqlx::Error| database_failure("lock urgent reconciliation", tenant_id, err))?
    .ok_or(UrgentStaffingErr::NotFound)?;
    let report_status: UrgentWorkStatus = UrgentWorkStatus::from_code(&context.report_status).ok_or_else(|| {
        error!(
            operation = "reconcile_urgent_work",
            tenant_id = %tenant_id,
            report_id = %report_id,
            report_status = %context.report_status,
            "Urgent-work report has an unsupported lifecycle status"
        );
        UrgentStaffingErr::BackendUnavailable
    })?;
    if report_status != UrgentWorkStatus::Completed {
        return Err(UrgentStaffingErr::Conflict);
    }
    let _staff_ended_at: DateTime<Utc> = context.staff_ended_at.ok_or(UrgentStaffingErr::Conflict)?;
    let staff_worked_seconds: i64 = context.staff_worked_seconds.ok_or(UrgentStaffingErr::Conflict)?;
    let confirmed_customer_id: Uuid = context
        .confirmed_customer_id
        .ok_or(UrgentStaffingErr::InvalidInput("customer evidence is required"))?;
    let confirmed_started_at: DateTime<Utc> = context
        .confirmed_started_at
        .ok_or(UrgentStaffingErr::InvalidInput("customer evidence is required"))?;
    let confirmed_ended_at: DateTime<Utc> = context
        .confirmed_ended_at
        .ok_or(UrgentStaffingErr::InvalidInput("customer evidence is required"))?;
    let confirmed_worked_seconds: i64 = context
        .confirmed_worked_seconds
        .ok_or(UrgentStaffingErr::InvalidInput("customer evidence is required"))?;
    let overlaps_existing_assignment: bool = sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM business_shift_assignments AS assignment
            JOIN business_staffing_shifts AS shift
              ON shift.tenant_id = assignment.tenant_id
             AND shift.id = assignment.shift_id
            WHERE assignment.tenant_id = $1
              AND assignment.employee_id = $2
              AND assignment.status <> 'cancelled'
              AND shift.starts_at < $3
              AND shift.ends_at > $4
        ) AS "exists!"
        "#,
        tenant_id,
        context.employee_id,
        confirmed_ended_at,
        confirmed_started_at,
    )
    .fetch_one(tran.connection())
    .await
    .map_err(|err| database_failure("validate urgent reconciliation assignment overlap", tenant_id, err))?;
    if overlaps_existing_assignment {
        return Err(UrgentStaffingErr::Conflict);
    }
    let customer_time_zone: String = context.customer_time_zone.ok_or(UrgentStaffingErr::NotFound)?;
    let has_discrepancy: bool = context.claimed_customer_id != confirmed_customer_id
        || input.final_customer_id != context.claimed_customer_id
        || input.final_customer_id != confirmed_customer_id
        || context.staff_started_at != confirmed_started_at
        || _staff_ended_at != confirmed_ended_at
        || staff_worked_seconds != confirmed_worked_seconds
        || input.worked_seconds != staff_worked_seconds
        || input.worked_seconds != confirmed_worked_seconds;
    if has_discrepancy && input.adjustment_reason.is_none() {
        return Err(UrgentStaffingErr::InvalidInput(
            "customer or time discrepancies require an adjustment reason",
        ));
    }
    let job: ExistsRow = sqlx::query_as!(
        ExistsRow,
        "SELECT EXISTS (SELECT 1 FROM business_staffing_jobs WHERE tenant_id = $1 AND id = $2 AND status = 'active') AS \"exists!\"",
        tenant_id,
        input.job_id,
    )
    .fetch_one(tran.connection())
    .await
    .map_err(|err: sqlx::Error| database_failure("validate urgent reconciliation job", tenant_id, err))?;
    if !job.exists {
        return Err(UrgentStaffingErr::NotFound);
    }
    let work_date_row: WorkDateRow = sqlx::query_as!(
        WorkDateRow,
        "SELECT ($1::TIMESTAMPTZ AT TIME ZONE $2)::DATE AS \"work_date!\"",
        confirmed_started_at,
        customer_time_zone,
    )
    .fetch_one(tran.connection())
    .await
    .map_err(|err: sqlx::Error| database_failure("derive urgent local work date", tenant_id, err))?;
    let work_date: NaiveDate = work_date_row.work_date;
    let financial_period_open = sqlx::query_scalar!(
        r#"SELECT shepherd_financial_date_is_open_for_update(
            $1,
            shepherd_current_branch_id(),
            $2
        ) AS "is_open!""#,
        tenant_id,
        work_date,
    )
    .fetch_one(tran.connection())
    .await
    .map_err(|err| database_failure("validate urgent reconciliation period", tenant_id, err))?;
    if !financial_period_open {
        return Err(UrgentStaffingErr::Conflict);
    }

    // This client treats every authorized staff member as staffing-eligible. Keep
    // the immutable snapshot column for compatibility, but do not require a
    // separate service-eligibility record or exception reason.
    let eligibility_exception_reason: Option<&str> = None;

    let (
        customer_bill_rate_id,
        worker_pay_rate_id,
        rate_source,
        manual_rate_reason,
        currency,
        bill_rate,
        worker_rate,
    ): (Option<Uuid>, Option<Uuid>, &str, Option<&str>, String, String, String) =
        match input.manual_rate.as_ref() {
            Some(ManualRateOverride {
                reason,
                currency,
                bill_hourly_rate,
                worker_hourly_rate,
            }) => (
                None,
                None,
                "manual",
                Some(reason.as_str()),
                currency.clone(),
                bill_hourly_rate.clone(),
                worker_hourly_rate.clone(),
            ),
            None => {
                let customer_bill_rate: ResolvedRateRow = sqlx::query_as!(
                    ResolvedRateRow,
                    r#"
                    SELECT id, currency, hourly_rate::TEXT AS "hourly_rate!"
                    FROM business_staffing_rates
                    WHERE tenant_id = $1
                      AND rate_kind = 'customer_bill'
                      AND customer_id = $2
                      AND (employee_id IS NULL OR employee_id = $3)
                      AND effective_from <= $4
                      AND (effective_to IS NULL OR effective_to >= $4)
                      AND is_active
                    ORDER BY
                        (employee_id IS NOT NULL) DESC,
                        priority DESC,
                        effective_from DESC,
                        id
                    LIMIT 1
                    "#,
                    tenant_id,
                    input.final_customer_id,
                    context.employee_id,
                    work_date,
                )
                .fetch_optional(tran.connection())
                .await
                .map_err(|err: sqlx::Error| database_failure("resolve urgent customer bill rate", tenant_id, err))?
                .ok_or(UrgentStaffingErr::MissingStaffingRate)?;
                let worker_pay_rate: ResolvedRateRow = sqlx::query_as!(
                    ResolvedRateRow,
                    r#"
                    SELECT id, currency, hourly_rate::TEXT AS "hourly_rate!"
                    FROM business_staffing_rates
                    WHERE tenant_id = $1
                      AND rate_kind = 'worker_pay'
                      AND (customer_id IS NULL OR customer_id = $2)
                      AND (employee_id IS NULL OR employee_id = $3)
                      AND effective_from <= $4
                      AND (effective_to IS NULL OR effective_to >= $4)
                      AND is_active
                    ORDER BY
                        (employee_id IS NOT NULL) DESC,
                        (customer_id IS NOT NULL) DESC,
                        priority DESC,
                        effective_from DESC,
                        id
                    LIMIT 1
                    "#,
                    tenant_id,
                    input.final_customer_id,
                    context.employee_id,
                    work_date,
                )
                .fetch_optional(tran.connection())
                .await
                .map_err(|err: sqlx::Error| database_failure("resolve urgent worker pay rate", tenant_id, err))?
                .ok_or(UrgentStaffingErr::MissingStaffingRate)?;
                if customer_bill_rate.currency != worker_pay_rate.currency {
                    warn!(
                        operation = "urgent_staffing.reconcile",
                        tenant_id = %tenant_id,
                        report_id = %report_id,
                        customer_bill_currency = %customer_bill_rate.currency,
                        worker_pay_currency = %worker_pay_rate.currency,
                        "Urgent customer bill and worker pay rates use different currencies"
                    );
                    return Err(UrgentStaffingErr::InvalidInput(
                        "customer bill and worker pay rates must use the same currency",
                    ));
                }
                (
                    Some(customer_bill_rate.id),
                    Some(worker_pay_rate.id),
                    "configured",
                    None,
                    customer_bill_rate.currency,
                    customer_bill_rate.hourly_rate,
                    worker_pay_rate.hourly_rate,
                )
            }
        };

    let shift_insert: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_staffing_shifts (
            id, tenant_id, customer_id, job_id,
            starts_at, ends_at, required_workers, status, notes,
            created_by_account_id, updated_by_account_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, 1, 'completed',
            'System-created after urgent work reconciliation', $7, $7
        )
        "#,
        shift_id,
        tenant_id,
        input.final_customer_id,
        input.job_id,
        confirmed_started_at,
        confirmed_ended_at,
        actor_account_id,
    )
    .execute(tran.connection())
    .await
    .map_err(|err: sqlx::Error| mutation_failure("create reconciled urgent shift", tenant_id, err))?;
    let assignment_insert: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_shift_assignments (
            id, tenant_id, shift_id, employee_id, urgent_work_report_id,
            customer_bill_rate_id, worker_pay_rate_id, rate_source, manual_rate_reason, currency,
            bill_hourly_rate_snapshot, worker_hourly_rate_snapshot,
            eligibility_exception_reason, status, worked_seconds, observed_worked_seconds, approval_adjustment_reason,
            customer_amount, worker_amount, margin_amount,
            approved_at, approved_by_account_id, created_by_account_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::TEXT::NUMERIC, $12::TEXT::NUMERIC,
            $13, 'approved', $14::BIGINT, $15::BIGINT, $16,
            ROUND($11::TEXT::NUMERIC * $14::BIGINT::NUMERIC / 3600, 4),
            ROUND($12::TEXT::NUMERIC * $14::BIGINT::NUMERIC / 3600, 4),
            ROUND($11::TEXT::NUMERIC * $14::BIGINT::NUMERIC / 3600, 4)
                - ROUND($12::TEXT::NUMERIC * $14::BIGINT::NUMERIC / 3600, 4),
            CURRENT_TIMESTAMP, $17, $17
        )
        "#,
        assignment_id,
        tenant_id,
        shift_id,
        context.employee_id,
        report_id,
        customer_bill_rate_id,
        worker_pay_rate_id,
        rate_source,
        manual_rate_reason,
        currency,
        bill_rate,
        worker_rate,
        eligibility_exception_reason,
        input.worked_seconds,
        staff_worked_seconds,
        input.adjustment_reason,
        actor_account_id,
    )
    .execute(tran.connection())
    .await
    .map_err(|err: sqlx::Error| mutation_failure("create reconciled urgent assignment", tenant_id, err))?;
    let copied_record_id: Uuid = Uuid::new_v4();
    let customer_copy: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_customer_work_records (
            id, tenant_id, assignment_id, confirmed_customer_id,
            confirmed_started_at, confirmed_ended_at, customer_reference, notes,
            recorded_by_account_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        copied_record_id,
        tenant_id,
        assignment_id,
        confirmed_customer_id,
        confirmed_started_at,
        confirmed_ended_at,
        context.customer_reference,
        context.customer_notes,
        actor_account_id,
    )
    .execute(tran.connection())
    .await
    .map_err(|err: sqlx::Error| mutation_failure("link urgent customer evidence to assignment", tenant_id, err))?;
    let report_update: PgQueryResult = sqlx::query!(
        "UPDATE business_urgent_work_reports SET status = 'reconciled', updated_at = CURRENT_TIMESTAMP WHERE tenant_id = $1 AND id = $2 AND status = 'completed'",
        tenant_id,
        report_id,
    )
    .execute(tran.connection())
    .await
    .map_err(|err: sqlx::Error| mutation_failure("finalize urgent work report", tenant_id, err))?;
    if report_update.rows_affected() != 1 {
        return Err(UrgentStaffingErr::Conflict);
    }
    trace!(tenant_id = %tenant_id, report_id = %report_id, shift_id = %shift_id, assignment_id = %assignment_id, shift_rows = shift_insert.rows_affected(), assignment_rows = assignment_insert.rows_affected(), customer_rows = customer_copy.rows_affected(), "Urgent work converted to approved staffing snapshot");
    let mut rows: Vec<ReconcileRow> = load_reconciliation_rows(
        tran.connection(),
        tenant_id,
        ReconcileRowQuery {
            report_id: Some(report_id),
            cursor_active: None,
            cursor_started_at: None,
            cursor_report_id: None,
            customer_id: None,
            confirmed: None,
            period_start: None,
            period_end: None,
            limit: 1,
        },
    )
    .await
    .map_err(|err: sqlx::Error| database_failure("load reconciled urgent work", tenant_id, err))?;
    let row: ReconcileRow = rows.pop().ok_or(UrgentStaffingErr::BackendUnavailable)?;
    reconciliation_from_row(row)
}

pub async fn enqueue_notification(
    tran: &mut TenantTransaction,
    tenant_id: Uuid,
    event_type: &str,
    aggregate_id: Uuid,
    report_id: Uuid,
) -> Result<(), UrgentStaffingErr> {
    let result: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO notification_outbox (
            id, tenant_id, branch_id, event_type, aggregate_id, channel, destination, message
        )
        SELECT gen_random_uuid(), $1, destination.branch_id, $2, $3, destination.channel, destination.destination,
               'Urgent staffing work updated; report ' || $4::UUID::TEXT
        FROM notification_destinations AS destination
        WHERE destination.tenant_id = $1 AND destination.enabled
        ON CONFLICT (tenant_id, branch_id, event_type, aggregate_id, channel, destination) DO NOTHING
        "#,
        tenant_id,
        event_type,
        aggregate_id,
        report_id,
    )
    .execute(tran.connection())
    .await
    .map_err(|err: sqlx::Error| database_failure("enqueue urgent work notification", tenant_id, err))?;
    trace!(tenant_id = %tenant_id, report_id = %report_id, event_type, destination_count = result.rows_affected(), "Urgent-work notifications enqueued");
    Ok(())
}

fn tenant_failure(op: &str, tenant_id: Uuid, err: sqlx::Error) -> UrgentStaffingErr {
    error!(op, tenant_id = %tenant_id, reason = %err, "Urgent-work tenant db operation failed");
    UrgentStaffingErr::BackendUnavailable
}

fn tenant_tran_failure(op: &str, tenant_id: Uuid, err: TenantDbErr) -> UrgentStaffingErr {
    error!(op, tenant_id = %tenant_id, reason = %err, "Urgent-work automatic tenant operation failed");
    UrgentStaffingErr::BackendUnavailable
}

fn database_failure(op: &str, tenant_id: Uuid, err: sqlx::Error) -> UrgentStaffingErr {
    error!(op, tenant_id = %tenant_id, reason = %err, "Urgent-work db operation failed");
    UrgentStaffingErr::BackendUnavailable
}

fn mutation_failure(op: &str, tenant_id: Uuid, err: sqlx::Error) -> UrgentStaffingErr {
    let mapped: UrgentStaffingErr = match &err {
        sqlx::Error::Database(database_error) if database_error.is_unique_violation() => UrgentStaffingErr::Conflict,
        sqlx::Error::Database(database_error) if database_error.code().as_deref() == Some("55000") => {
            UrgentStaffingErr::Conflict
        }
        sqlx::Error::Database(database_error)
            if database_error.is_check_violation() || database_error.is_foreign_key_violation() =>
        {
            UrgentStaffingErr::InvalidInput("urgent work violates a db constraint")
        }
        _ => UrgentStaffingErr::BackendUnavailable,
    };
    if matches!(mapped, UrgentStaffingErr::BackendUnavailable) {
        error!(op, tenant_id = %tenant_id, reason = %err, "Urgent-work mutation failed unexpectedly");
    } else {
        warn!(op, tenant_id = %tenant_id, reason = %err, "Urgent-work mutation rejected by db invariant");
    }
    mapped
}
