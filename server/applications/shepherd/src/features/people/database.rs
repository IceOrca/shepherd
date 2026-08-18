use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use tracing::{error, warn, info, debug, trace};
use crate::features::people::core::{
    AttendanceSession, Department, DepartmentInput, Employee, EmployeeAssignment, EmployeeAssignmentInput,
    EmployeeInput, EmployeeStatus, HrError, HrRecordStatus, JobPosition, JobPositionInput, PeopleRepo,
};
use uuid::Uuid;

use infra_postgres::{DatabaseAdapter, TenantTransaction};

pub struct PeopleProvider {
    database: Arc<DatabaseAdapter>,
}

impl PeopleProvider {
    pub fn new_arc(database: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { database })
    }

    async fn begin_active_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, HrError> {
        self.database.begin_tenant(tenant_id).await.map_err(|error| {
            error!("HR tenant transaction failed: tenant_id={} error={}", tenant_id, error);
            HrError::BackendUnavailable
        })
    }
}

#[derive(Debug)]
struct EmployeeRow {
    id: Uuid,
    account_id: Option<Uuid>,
    employee_code: String,
    display_name: String,
    work_email: Option<String>,
    work_phone: Option<String>,
    badge_id: Option<String>,
    status: String,
    hire_date: NaiveDate,
    termination_date: Option<NaiveDate>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<EmployeeRow> for Employee {
    type Error = HrError;

    fn try_from(row: EmployeeRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            account_id: row.account_id,
            employee_code: row.employee_code,
            display_name: row.display_name,
            work_email: row.work_email,
            work_phone: row.work_phone,
            badge_id: row.badge_id,
            status: EmployeeStatus::from_code(&row.status).ok_or(HrError::BackendUnavailable)?,
            hire_date: row.hire_date,
            termination_date: row.termination_date,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct DepartmentRow {
    id: Uuid,
    code: String,
    name: String,
    parent_department_id: Option<Uuid>,
    manager_employee_id: Option<Uuid>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<DepartmentRow> for Department {
    type Error = HrError;

    fn try_from(row: DepartmentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            code: row.code,
            name: row.name,
            parent_department_id: row.parent_department_id,
            manager_employee_id: row.manager_employee_id,
            status: HrRecordStatus::from_code(&row.status).ok_or(HrError::BackendUnavailable)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct JobRow {
    id: Uuid,
    code: String,
    name: String,
    department_id: Option<Uuid>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<JobRow> for JobPosition {
    type Error = HrError;

    fn try_from(row: JobRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            code: row.code,
            name: row.name,
            department_id: row.department_id,
            status: HrRecordStatus::from_code(&row.status).ok_or(HrError::BackendUnavailable)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct AssignmentRow {
    id: Uuid,
    employee_id: Uuid,
    branch_id: Uuid,
    facility_id: Option<Uuid>,
    department_id: Option<Uuid>,
    job_id: Option<Uuid>,
    manager_employee_id: Option<Uuid>,
    date_start: NaiveDate,
    date_end: Option<NaiveDate>,
    is_primary: bool,
    created_at: DateTime<Utc>,
}

#[derive(Debug)]
struct AttendanceSessionRow {
    id: Uuid,
    employee_id: Uuid,
    facility_id: Uuid,
    check_in_at: DateTime<Utc>,
    check_out_at: Option<DateTime<Utc>>,
    worked_seconds: Option<i64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<AttendanceSessionRow> for AttendanceSession {
    fn from(row: AttendanceSessionRow) -> Self {
        Self {
            id: row.id,
            employee_id: row.employee_id,
            facility_id: row.facility_id,
            check_in_at: row.check_in_at,
            check_out_at: row.check_out_at,
            worked_seconds: row.worked_seconds,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<AssignmentRow> for EmployeeAssignment {
    fn from(row: AssignmentRow) -> Self {
        Self {
            id: row.id,
            employee_id: row.employee_id,
            branch_id: row.branch_id,
            facility_id: row.facility_id,
            department_id: row.department_id,
            job_id: row.job_id,
            manager_employee_id: row.manager_employee_id,
            date_start: row.date_start,
            date_end: row.date_end,
            is_primary: row.is_primary,
            created_at: row.created_at,
        }
    }
}

#[async_trait]
impl PeopleRepo for PeopleProvider {
    async fn list_employees(&self, tenant_id: Uuid) -> Result<Vec<Employee>, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let rows: Vec<EmployeeRow> = sqlx::query_as!(
            EmployeeRow,
            r#"
            SELECT id, account_id, employee_code, display_name, work_email, work_phone, badge_id,
                   status, hire_date, termination_date, created_at, updated_at
            FROM hr_employees
            WHERE tenant_id = $1
            ORDER BY lower(display_name), employee_code
            "#,
            tenant_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list employees", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit employee list", tenant_id, error))?;
        info!(
            "Tenant employee directory loaded: tenant_id={} employees={}",
            tenant_id,
            rows.len()
        );
        rows.into_iter().map(Employee::try_from).collect()
    }

    async fn find_employee(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<Option<Employee>, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let row: Option<EmployeeRow> = sqlx::query_as!(
            EmployeeRow,
            r#"
            SELECT id, account_id, employee_code, display_name, work_email, work_phone, badge_id,
                   status, hire_date, termination_date, created_at, updated_at
            FROM hr_employees
            WHERE tenant_id = $1 AND id = $2
            "#,
            tenant_id,
            employee_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| database_failure("find employee", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit employee lookup", tenant_id, error))?;
        row.map(Employee::try_from).transpose()
    }

    async fn find_employee_by_account(&self, tenant_id: Uuid, account_id: Uuid) -> Result<Option<Employee>, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let row: Option<EmployeeRow> = sqlx::query_as!(
            EmployeeRow,
            r#"
            SELECT id, account_id, employee_code, display_name, work_email, work_phone, badge_id,
                   status, hire_date, termination_date, created_at, updated_at
            FROM hr_employees
            WHERE tenant_id = $1 AND account_id = $2
            "#,
            tenant_id,
            account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| database_failure("find employee by account", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit employee account lookup", tenant_id, error))?;
        row.map(Employee::try_from).transpose()
    }

    async fn create_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let row: EmployeeRow = sqlx::query_as!(
            EmployeeRow,
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, account_id, employee_code, display_name, work_email, work_phone, badge_id,
                status, hire_date, termination_date, created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
            RETURNING id, account_id, employee_code, display_name, work_email, work_phone, badge_id,
                      status, hire_date, termination_date, created_at, updated_at
            "#,
            employee_id,
            tenant_id,
            input.account_id,
            input.employee_code,
            input.display_name,
            input.work_email,
            input.work_phone,
            input.badge_id,
            input.status.as_code(),
            input.hire_date,
            input.termination_date,
            audit_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create employee", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit employee creation", tenant_id, error))?;
        info!(
            "Employee created: tenant_id={} employee_id={} employee_code={} linked_account_id={:?} audit_account_id={}",
            tenant_id, employee_id, input.employee_code, input.account_id, audit_account_id
        );
        Employee::try_from(row)
    }

    async fn update_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        if input.status == EmployeeStatus::Terminated {
            let termination_date = input.termination_date.ok_or(HrError::InvalidInput(
                "terminated employee is missing a termination date",
            ))?;
            let has_later_assignment: bool = sqlx::query_scalar!(
                r#"
                SELECT
                    EXISTS (
                        SELECT 1
                        FROM hr_employee_assignments
                        WHERE tenant_id = $1 AND employee_id = $2 AND date_start > $3
                    )
                    OR EXISTS (
                        SELECT 1
                        FROM hr_employee_schedule_assignments
                        WHERE tenant_id = $1 AND employee_id = $2 AND date_start > $3
                    ) AS "exists!"
                "#,
                tenant_id,
                employee_id,
                termination_date,
            )
            .fetch_one(transaction.connection())
            .await
            .map_err(|error| database_failure("validate employee termination date", tenant_id, error))?;
            if has_later_assignment {
                return Err(HrError::Conflict);
            }
        }
        let row: Option<EmployeeRow> = sqlx::query_as!(
            EmployeeRow,
            r#"
            UPDATE hr_employees
            SET account_id = $3,
                employee_code = $4,
                display_name = $5,
                work_email = $6,
                work_phone = $7,
                badge_id = $8,
                status = $9,
                hire_date = $10,
                termination_date = $11,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $12
            WHERE tenant_id = $1 AND id = $2
            RETURNING id, account_id, employee_code, display_name, work_email, work_phone, badge_id,
                      status, hire_date, termination_date, created_at, updated_at
            "#,
            tenant_id,
            employee_id,
            input.account_id,
            input.employee_code,
            input.display_name,
            input.work_email,
            input.work_phone,
            input.badge_id,
            input.status.as_code(),
            input.hire_date,
            input.termination_date,
            audit_account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| mutation_failure("update employee", tenant_id, error))?;
        let row: EmployeeRow = row.ok_or(HrError::NotFound)?;
        if let Some(termination_date) = input.termination_date {
            sqlx::query!(
                r#"
                UPDATE hr_employee_assignments
                SET date_end = $3
                WHERE tenant_id = $1
                  AND employee_id = $2
                  AND date_end IS NULL
                  AND date_start <= $3
                "#,
                tenant_id,
                employee_id,
                termination_date,
            )
            .execute(transaction.connection())
            .await
            .map_err(|error| database_failure("close assignments for terminated employee", tenant_id, error))?;
            sqlx::query!(
                r#"
                UPDATE hr_employee_schedule_assignments
                SET date_end = $3
                WHERE tenant_id = $1
                  AND employee_id = $2
                  AND date_end IS NULL
                  AND date_start <= $3
                "#,
                tenant_id,
                employee_id,
                termination_date,
            )
            .execute(transaction.connection())
            .await
            .map_err(|error| {
                database_failure(
                    "close working schedule assignments for terminated employee",
                    tenant_id,
                    error,
                )
            })?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit employee update", tenant_id, error))?;
        info!(
            "Employee updated: tenant_id={} employee_id={} status={} audit_account_id={}",
            tenant_id,
            employee_id,
            input.status.as_code(),
            audit_account_id
        );
        Employee::try_from(row)
    }

    async fn list_departments(&self, tenant_id: Uuid) -> Result<Vec<Department>, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let rows: Vec<DepartmentRow> = sqlx::query_as!(
            DepartmentRow,
            r#"
            SELECT id, code, name, parent_department_id, manager_employee_id, status, created_at, updated_at
            FROM hr_departments
            WHERE tenant_id = $1
            ORDER BY lower(name), code
            "#,
            tenant_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list departments", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit department list", tenant_id, error))?;
        rows.into_iter().map(Department::try_from).collect()
    }

    async fn create_department(
        &self,
        tenant_id: Uuid,
        department_id: Uuid,
        input: &DepartmentInput,
        audit_account_id: Uuid,
    ) -> Result<Department, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let row: DepartmentRow = sqlx::query_as!(
            DepartmentRow,
            r#"
            INSERT INTO hr_departments (
                id, tenant_id, code, name, parent_department_id, manager_employee_id, status,
                created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
            RETURNING id, code, name, parent_department_id, manager_employee_id, status, created_at, updated_at
            "#,
            department_id,
            tenant_id,
            input.code,
            input.name,
            input.parent_department_id,
            input.manager_employee_id,
            input.status.as_code(),
            audit_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create department", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit department creation", tenant_id, error))?;
        info!(
            "HR department created: tenant_id={} department_id={} code={} audit_account_id={}",
            tenant_id, department_id, input.code, audit_account_id
        );
        Department::try_from(row)
    }

    async fn update_department(
        &self,
        tenant_id: Uuid,
        department_id: Uuid,
        input: &DepartmentInput,
        audit_account_id: Uuid,
    ) -> Result<Department, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        if let Some(parent_id) = input.parent_department_id {
            let creates_cycle: bool = sqlx::query_scalar!(
                r#"
                WITH RECURSIVE descendants AS (
                    SELECT id
                    FROM hr_departments
                    WHERE tenant_id = $1 AND parent_department_id = $2
                    UNION ALL
                    SELECT department.id
                    FROM hr_departments AS department
                    JOIN descendants ON department.parent_department_id = descendants.id
                    WHERE department.tenant_id = $1
                )
                SELECT EXISTS (SELECT 1 FROM descendants WHERE id = $3) AS "exists!"
                "#,
                tenant_id,
                department_id,
                parent_id,
            )
            .fetch_one(transaction.connection())
            .await
            .map_err(|error| database_failure("validate department hierarchy", tenant_id, error))?;
            if creates_cycle {
                return Err(HrError::InvalidInput("department hierarchy would contain a cycle"));
            }
        }
        let row: Option<DepartmentRow> = sqlx::query_as!(
            DepartmentRow,
            r#"
            UPDATE hr_departments
            SET code = $3,
                name = $4,
                parent_department_id = $5,
                manager_employee_id = $6,
                status = $7,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $8
            WHERE tenant_id = $1 AND id = $2
            RETURNING id, code, name, parent_department_id, manager_employee_id, status, created_at, updated_at
            "#,
            tenant_id,
            department_id,
            input.code,
            input.name,
            input.parent_department_id,
            input.manager_employee_id,
            input.status.as_code(),
            audit_account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| mutation_failure("update department", tenant_id, error))?;
        let row = row.ok_or(HrError::NotFound)?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit department update", tenant_id, error))?;
        info!(
            "HR department updated: tenant_id={} department_id={} status={} audit_account_id={}",
            tenant_id,
            department_id,
            input.status.as_code(),
            audit_account_id
        );
        Department::try_from(row)
    }

    async fn list_jobs(&self, tenant_id: Uuid) -> Result<Vec<JobPosition>, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let rows: Vec<JobRow> = sqlx::query_as!(
            JobRow,
            r#"
            SELECT id, code, name, department_id, status, created_at, updated_at
            FROM hr_jobs
            WHERE tenant_id = $1
            ORDER BY lower(name), code
            "#,
            tenant_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list jobs", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit job list", tenant_id, error))?;
        rows.into_iter().map(JobPosition::try_from).collect()
    }

    async fn create_job(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        input: &JobPositionInput,
        audit_account_id: Uuid,
    ) -> Result<JobPosition, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let row: JobRow = sqlx::query_as!(
            JobRow,
            r#"
            INSERT INTO hr_jobs (
                id, tenant_id, code, name, department_id, status, created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
            RETURNING id, code, name, department_id, status, created_at, updated_at
            "#,
            job_id,
            tenant_id,
            input.code,
            input.name,
            input.department_id,
            input.status.as_code(),
            audit_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create job", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit job creation", tenant_id, error))?;
        info!(
            "HR job position created: tenant_id={} job_id={} code={} audit_account_id={}",
            tenant_id, job_id, input.code, audit_account_id
        );
        JobPosition::try_from(row)
    }

    async fn update_job(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        input: &JobPositionInput,
        audit_account_id: Uuid,
    ) -> Result<JobPosition, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let row: Option<JobRow> = sqlx::query_as!(
            JobRow,
            r#"
            UPDATE hr_jobs
            SET code = $3,
                name = $4,
                department_id = $5,
                status = $6,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $7
            WHERE tenant_id = $1 AND id = $2
            RETURNING id, code, name, department_id, status, created_at, updated_at
            "#,
            tenant_id,
            job_id,
            input.code,
            input.name,
            input.department_id,
            input.status.as_code(),
            audit_account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| mutation_failure("update job", tenant_id, error))?;
        let row = row.ok_or(HrError::NotFound)?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit job update", tenant_id, error))?;
        info!(
            "HR job position updated: tenant_id={} job_id={} status={} audit_account_id={}",
            tenant_id,
            job_id,
            input.status.as_code(),
            audit_account_id
        );
        JobPosition::try_from(row)
    }

    async fn list_assignments(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<Vec<EmployeeAssignment>, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let employee_exists: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2) AS "exists!""#,
            tenant_id,
            employee_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("validate employee for assignment list", tenant_id, error))?;
        if !employee_exists {
            return Err(HrError::NotFound);
        }
        let rows: Vec<AssignmentRow> = sqlx::query_as!(
            AssignmentRow,
            r#"
            SELECT id, employee_id, branch_id, facility_id, department_id, job_id, manager_employee_id,
                   date_start, date_end, is_primary, created_at
            FROM hr_employee_assignments
            WHERE tenant_id = $1 AND employee_id = $2
            ORDER BY date_start DESC, created_at DESC
            "#,
            tenant_id,
            employee_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list employee assignments", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit assignment list", tenant_id, error))?;
        Ok(rows.into_iter().map(EmployeeAssignment::from).collect())
    }

    async fn create_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeAssignment, HrError> {
        let mut transaction = self.begin_active_tenant(tenant_id).await?;
        let employee_exists: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2) AS "exists!""#,
            tenant_id,
            employee_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("validate employee for assignment creation", tenant_id, error))?;
        if !employee_exists {
            return Err(HrError::NotFound);
        }
        if input.is_primary {
            sqlx::query!(
                r#"
                UPDATE hr_employee_assignments
                SET date_end = $3::date - 1
                WHERE tenant_id = $1
                  AND employee_id = $2
                  AND is_primary
                  AND date_end IS NULL
                  AND date_start < $3
                "#,
                tenant_id,
                employee_id,
                input.date_start,
            )
            .execute(transaction.connection())
            .await
            .map_err(|error| database_failure("close previous primary assignment", tenant_id, error))?;

            let overlaps: bool = sqlx::query_scalar!(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM hr_employee_assignments
                    WHERE tenant_id = $1
                      AND employee_id = $2
                      AND is_primary
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
            .map_err(|error| database_failure("check primary assignment overlap", tenant_id, error))?;
            if overlaps {
                info!(
                    "Primary assignment rejected because dates overlap: tenant_id={} employee_id={} date_start={} date_end={:?}",
                    tenant_id, employee_id, input.date_start, input.date_end
                );
                return Err(HrError::Conflict);
            }
        }

        let row: AssignmentRow = sqlx::query_as!(
            AssignmentRow,
            r#"
            INSERT INTO hr_employee_assignments (
                id, tenant_id, employee_id, branch_id, facility_id, department_id, job_id, manager_employee_id,
                date_start, date_end, is_primary, created_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING id, employee_id, branch_id, facility_id, department_id, job_id, manager_employee_id,
                      date_start, date_end, is_primary, created_at
            "#,
            assignment_id,
            tenant_id,
            employee_id,
            input.branch_id,
            input.facility_id,
            input.department_id,
            input.job_id,
            input.manager_employee_id,
            input.date_start,
            input.date_end,
            input.is_primary,
            audit_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create employee assignment", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit assignment creation", tenant_id, error))?;
        info!(
            "Employee assignment created: tenant_id={} employee_id={} assignment_id={} branch_id={} facility_id={:?} date_start={} date_end={:?} primary={} audit_account_id={}",
            tenant_id,
            employee_id,
            assignment_id,
            input.branch_id,
            input.facility_id,
            input.date_start,
            input.date_end,
            input.is_primary,
            audit_account_id
        );
        Ok(row.into())
    }

    async fn list_attendance_sessions(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<AttendanceSession>, HrError> {
        let mut transaction: TenantTransaction = self.begin_active_tenant(tenant_id).await?;
        let employee_exists: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2) AS "exists!""#,
            tenant_id,
            employee_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("validate employee for attendance list", tenant_id, error))?;
        if !employee_exists {
            return Err(HrError::NotFound);
        }
        let rows: Vec<AttendanceSessionRow> = sqlx::query_as!(
            AttendanceSessionRow,
            r#"
            SELECT id, employee_id, facility_id, check_in_at, check_out_at, worked_seconds, created_at, updated_at
            FROM hr_attendance_sessions
            WHERE tenant_id = $1 AND employee_id = $2
            ORDER BY check_in_at DESC, id DESC
            "#,
            tenant_id,
            employee_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list attendance sessions", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit attendance session list", tenant_id, error))?;
        Ok(rows.into_iter().map(AttendanceSession::from).collect())
    }

    async fn check_in(
        &self,
        tenant_id: Uuid,
        attendance_session_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
        facility_id: Uuid,
    ) -> Result<AttendanceSession, HrError> {
        let mut transaction: TenantTransaction = self.begin_active_tenant(tenant_id).await?;
        let row: Option<AttendanceSessionRow> = sqlx::query_as!(
            AttendanceSessionRow,
            r#"
            INSERT INTO hr_attendance_sessions (id, tenant_id, employee_id, facility_id, check_in_by_account_id)
            SELECT $1, $2, employee.id, facility.id, $4
            FROM hr_employees AS employee
            INNER JOIN facilities AS facility
                ON facility.tenant_id = employee.tenant_id
               AND facility.id = $5
               AND facility.status = 'active'
            INNER JOIN branches AS branch
                ON branch.tenant_id = facility.tenant_id
               AND branch.id = facility.branch_id
               AND branch.status = 'active'
            WHERE employee.tenant_id = $2
              AND employee.id = $3
              AND employee.account_id = $4
              AND employee.status = 'active'
            RETURNING id, employee_id, facility_id, check_in_at, check_out_at, worked_seconds, created_at, updated_at
            "#,
            attendance_session_id,
            tenant_id,
            employee_id,
            account_id,
            facility_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| mutation_failure("check in employee", tenant_id, error))?;
        let row: AttendanceSessionRow = row.ok_or(HrError::NotFound)?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit employee check in", tenant_id, error))?;
        info!(
            "Employee checked in: tenant_id={} employee_id={} attendance_session_id={} account_id={} facility_id={}",
            tenant_id, employee_id, attendance_session_id, account_id, facility_id
        );
        Ok(row.into())
    }

    async fn check_out(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
    ) -> Result<AttendanceSession, HrError> {
        let mut transaction: TenantTransaction = self.begin_active_tenant(tenant_id).await?;
        let row: Option<AttendanceSessionRow> = sqlx::query_as!(
            AttendanceSessionRow,
            r#"
            UPDATE hr_attendance_sessions AS attendance
            SET check_out_at = CURRENT_TIMESTAMP,
                check_out_by_account_id = $3,
                updated_at = CURRENT_TIMESTAMP
            FROM hr_employees AS employee
            WHERE attendance.tenant_id = $1
              AND attendance.employee_id = $2
              AND attendance.check_out_at IS NULL
              AND employee.tenant_id = attendance.tenant_id
              AND employee.id = attendance.employee_id
              AND employee.account_id = $3
            RETURNING attendance.id, attendance.employee_id, attendance.facility_id, attendance.check_in_at,
                      attendance.check_out_at, attendance.worked_seconds, attendance.created_at,
                      attendance.updated_at
            "#,
            tenant_id,
            employee_id,
            account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| mutation_failure("check out employee", tenant_id, error))?;
        let row: AttendanceSessionRow = row.ok_or(HrError::NotFound)?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit employee check out", tenant_id, error))?;
        info!(
            "Employee checked out: tenant_id={} employee_id={} attendance_session_id={} account_id={}",
            tenant_id, employee_id, row.id, account_id
        );
        Ok(row.into())
    }
}

fn database_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> HrError {
    error!(
        "HR database operation failed: operation={} tenant_id={} error={}",
        operation, tenant_id, error
    );
    HrError::BackendUnavailable
}

fn mutation_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> HrError {
    let mapped = error
        .as_database_error()
        .map_or(HrError::BackendUnavailable, |database_error| {
            if database_error.is_unique_violation() {
                HrError::Conflict
            } else if database_error.is_foreign_key_violation() || database_error.is_check_violation() {
                HrError::InvalidInput("a referenced HR record is invalid")
            } else {
                HrError::BackendUnavailable
            }
        });
    error!(
        "HR mutation failed: operation={} tenant_id={} mapped_error={:?} error={}",
        operation, tenant_id, mapped, error
    );
    mapped
}
