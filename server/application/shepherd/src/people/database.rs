use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use tracing::{error, warn, info, debug, trace};
use crate::people::core::{
    AttendanceCursor, AttendancePage, AttendanceSession, Employee, EmployeeCitizenIdInput, EmployeeCursor,
    EmployeeInput, EmployeePage, EmployeeSensitiveProfile, EmployeeStatus, Gender, PeopleOpsErr,
};
use crate::people::security::{CitizenIdProtector, ProtectedCitizenId};
use uuid::Uuid;

use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use sqlx::PgConnection;
use super::security::CitizenIdProtectErr;

pub struct PeopleRepo {
    db: Arc<DatabaseAdapter>,
    citizen_id_protector: CitizenIdProtector,
}

#[derive(Debug)]
struct EmployeeRow {
    id: Uuid,
    branch_id: Uuid,
    account_id: Option<Uuid>,
    employee_code: String,
    display_name: String,
    legal_first_name: Option<String>,
    legal_middle_name: Option<String>,
    legal_last_name: Option<String>,
    personal_phone_e164: Option<String>,
    gender: Option<String>,
    citizen_id_country_code: Option<String>,
    citizen_id_last4: Option<String>,
    profile_complete: bool,
    status: String,
    hire_date: NaiveDate,
    termination_date: Option<NaiveDate>,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug)]
struct EmployeeSensitiveRow {
    employee_id: Uuid,
    citizen_id_country_code: Option<String>,
    citizen_id_key_id: Option<String>,
    citizen_id_ciphertext: Option<Vec<u8>>,
    version: i64,
}

#[derive(Debug)]
struct EmployeeSensitiveUpdateRow {
    branch_id: Uuid,
    citizen_id_country_code: Option<String>,
    citizen_id_last4: Option<String>,
    version: i64,
}

impl TryFrom<EmployeeRow> for Employee {
    type Error = PeopleOpsErr;

    fn try_from(row: EmployeeRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            branch_id: row.branch_id,
            account_id: row.account_id,
            employee_code: row.employee_code,
            display_name: row.display_name,
            legal_first_name: row.legal_first_name,
            legal_middle_name: row.legal_middle_name,
            legal_last_name: row.legal_last_name,
            personal_phone_e164: row.personal_phone_e164,
            gender: match row.gender.as_deref() {
                Some(code) => Some(Gender::from_code(code).ok_or(PeopleOpsErr::BackendUnavailable)?),
                None => None,
            },
            citizen_id_country_code: row.citizen_id_country_code,
            citizen_id_last4: row.citizen_id_last4,
            profile_complete: row.profile_complete,
            status: EmployeeStatus::from_code(&row.status).ok_or(PeopleOpsErr::BackendUnavailable)?,
            hire_date: row.hire_date,
            termination_date: row.termination_date,
            version: row.version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct AttendanceSessionRow {
    id: Uuid,
    employee_id: Uuid,
    branch_id: Uuid,
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
            branch_id: row.branch_id,
            check_in_at: row.check_in_at,
            check_out_at: row.check_out_at,
            worked_seconds: row.worked_seconds,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl PeopleRepo {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        let citizen_id_protector: CitizenIdProtector =
            CitizenIdProtector::from_env().unwrap_or_else(|err: CitizenIdProtectErr| {
                panic!("employee citizen-ID protection configuration is invalid: {err}")
            });
        Arc::new(Self {
            db,
            citizen_id_protector,
        })
    }

    pub async fn begin_active_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, PeopleOpsErr> {
        self.db.begin_tenant(tenant_id).await.map_err(|err: TenantDbErr| {
            error!("HR tenant tran failed: tenant_id={} err={}", tenant_id, err);
            PeopleOpsErr::BackendUnavailable
        })
    }

    fn sensitive_profile(
        &self,
        tenant_id: Uuid,
        row: EmployeeSensitiveRow,
    ) -> Result<EmployeeSensitiveProfile, PeopleOpsErr> {
        let citizen_id: Option<String> = match (
            row.citizen_id_country_code.as_deref(),
            row.citizen_id_key_id.as_deref(),
            row.citizen_id_ciphertext.as_deref(),
        ) {
            (None, None, None) => None,
            (Some(country_code), Some(key_id), Some(ciphertext)) => Some(
                self.citizen_id_protector
                    .reveal(tenant_id, country_code, key_id, ciphertext)
                    .map_err(|err: CitizenIdProtectErr| {
                        error!(
                            tenant_id = %tenant_id,
                            employee_id = %row.employee_id,
                            reason = %err,
                            "Employee citizen ID could not be decrypted"
                        );
                        PeopleOpsErr::BackendUnavailable
                    })?,
            ),
            _ => {
                error!(
                    tenant_id = %tenant_id,
                    employee_id = %row.employee_id,
                    "Employee citizen-ID storage is internally inconsistent"
                );
                return Err(PeopleOpsErr::BackendUnavailable);
            }
        };
        Ok(EmployeeSensitiveProfile {
            employee_id: row.employee_id,
            citizen_id_country_code: row.citizen_id_country_code,
            citizen_id,
            version: row.version,
        })
    }

    pub async fn list_employees(
        &self,
        tenant_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&EmployeeCursor>,
    ) -> Result<EmployeePage, PeopleOpsErr> {
        let normalized_search: Option<String> = search.map(str::to_owned);
        let cursor_name: Option<String> = cursor.map(|value: &EmployeeCursor| value.normalized_display_name.clone());
        let cursor_code: Option<String> = cursor.map(|value: &EmployeeCursor| value.employee_code.clone());
        let cursor_id: Option<Uuid> = cursor.map(|value: &EmployeeCursor| value.employee_id);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<EmployeeRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                sqlx::query_as!(
                    EmployeeRow,
                    r#"
                    SELECT id, branch_id, account_id, employee_code, display_name,
                           legal_first_name, legal_middle_name, legal_last_name,
                           personal_phone_e164, gender,
                           citizen_id_country_code, citizen_id_last4,
                           (legal_first_name IS NOT NULL AND legal_last_name IS NOT NULL) AS "profile_complete!",
                           status, hire_date, termination_date, version, created_at, updated_at
                    FROM hr_employees
                    WHERE tenant_id = $1
                      AND ($2::TEXT IS NULL
                           OR lower(display_name) LIKE '%' || $2 || '%'
                           OR lower(employee_code) LIKE '%' || $2 || '%'
                           OR lower(COALESCE(personal_phone_e164, '')) LIKE '%' || $2 || '%')
                      AND ($3::TEXT IS NULL
                           OR (lower(display_name), employee_code, id) > ($3, $4::TEXT, $5::UUID))
                    ORDER BY lower(display_name), employee_code, id
                    LIMIT $6
                    "#,
                    tenant_id,
                    normalized_search,
                    cursor_name,
                    cursor_code,
                    cursor_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("list employees", tenant_id, err))?;

        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<EmployeeCursor> = if has_more {
            rows.last().map(|row: &EmployeeRow| EmployeeCursor {
                normalized_display_name: row.display_name.to_lowercase(),
                employee_code: row.employee_code.clone(),
                employee_id: row.id,
            })
        } else {
            None
        };
        info!(tenant_id = %tenant_id, employee_count = rows.len(), has_more, "Tenant employee page loaded");
        let items: Vec<Employee> = rows
            .into_iter()
            .map(Employee::try_from)
            .collect::<Result<Vec<Employee>, PeopleOpsErr>>()?;
        Ok(EmployeePage { items, next_cursor })
    }

    pub async fn find_employee(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<Option<Employee>, PeopleOpsErr> {
        let row: Option<EmployeeRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                sqlx::query_as!(
                    EmployeeRow,
                    r#"
                    SELECT id, branch_id, account_id, employee_code, display_name,
                        legal_first_name, legal_middle_name, legal_last_name,
                        personal_phone_e164, gender,
                        citizen_id_country_code, citizen_id_last4,
                        (legal_first_name IS NOT NULL AND legal_last_name IS NOT NULL) AS "profile_complete!",
                        status, hire_date, termination_date, version, created_at, updated_at
                    FROM hr_employees
                    WHERE tenant_id = $1 AND id = $2
                    "#,
                    tenant_id,
                    employee_id,
                )
                .fetch_optional(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("find employee", tenant_id, err))?;
        row.map(Employee::try_from).transpose()
    }

    pub async fn find_employee_by_account(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
    ) -> Result<Option<Employee>, PeopleOpsErr> {
        let row: Option<EmployeeRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                sqlx::query_as!(
                    EmployeeRow,
                    r#"
                    SELECT id, branch_id, account_id, employee_code, display_name,
                           legal_first_name, legal_middle_name, legal_last_name,
                           personal_phone_e164, gender,
                           citizen_id_country_code, citizen_id_last4,
                           (legal_first_name IS NOT NULL AND legal_last_name IS NOT NULL) AS "profile_complete!",
                           status, hire_date, termination_date, version, created_at, updated_at
                    FROM hr_employees
                    WHERE tenant_id = $1 AND account_id = $2
                    "#,
                    tenant_id,
                    account_id,
                )
                .fetch_optional(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("find employee by account", tenant_id, err))?;
        row.map(Employee::try_from).transpose()
    }

    pub async fn create_employee(
        &self,
        tenant_id: Uuid,
        branch_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, PeopleOpsErr> {
        let row: EmployeeRow = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                sqlx::query_as!(
                    EmployeeRow,
                    r#"
                    INSERT INTO hr_employees (
                        id, tenant_id, branch_id, account_id, employee_code, display_name,
                        legal_first_name, legal_middle_name, legal_last_name,
                        personal_phone_e164, gender,
                        status, hire_date, termination_date, created_by_account_id, updated_by_account_id
                    )
                    VALUES (
                        $1, $2, $3, $4, $5, $6,
                        $7, $8, $9,
                        $10, $11,
                        $12, $13, $14, $15, $15
                    )
                    RETURNING id, branch_id, account_id, employee_code, display_name,
                              legal_first_name, legal_middle_name, legal_last_name,
                              personal_phone_e164, gender,
                              citizen_id_country_code, citizen_id_last4,
                              (legal_first_name IS NOT NULL AND legal_last_name IS NOT NULL) AS "profile_complete!",
                              status, hire_date, termination_date, version, created_at, updated_at
                    "#,
                    employee_id,
                    tenant_id,
                    branch_id,
                    input.account_id,
                    input.employee_code,
                    input.display_name,
                    input.legal_first_name,
                    input.legal_middle_name,
                    input.legal_last_name,
                    input.personal_phone_e164,
                    input.gender.map(|gender: Gender| gender.as_code()),
                    input.status.as_code(),
                    input.hire_date,
                    input.termination_date,
                    audit_account_id,
                )
                .fetch_one(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_mutation_failure("create employee", tenant_id, err))?;
        info!(
            "Employee created: tenant_id={} employee_id={} employee_code={} linked_account_id={:?} audit_account_id={}",
            tenant_id, employee_id, input.employee_code, input.account_id, audit_account_id
        );
        Employee::try_from(row)
    }

    pub async fn update_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, PeopleOpsErr> {
        let expected_version: i64 = input.expected_version.ok_or(PeopleOpsErr::InvalidInput(
            "employee update requires an expected version",
        ))?;
        let mut tran: TenantTransaction = self.begin_active_tenant(tenant_id).await?;
        let row: Option<EmployeeRow> = sqlx::query_as!(
            EmployeeRow,
            r#"
            UPDATE hr_employees
            SET account_id = $3,
                employee_code = $4,
                display_name = $5,
                legal_first_name = $6,
                legal_middle_name = $7,
                legal_last_name = $8,
                personal_phone_e164 = $9,
                gender = $10,
                status = $11,
                hire_date = $12,
                termination_date = $13,
                version = version + 1,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $14
            WHERE tenant_id = $1 AND id = $2 AND version = $15
            RETURNING id, branch_id, account_id, employee_code, display_name,
                legal_first_name, legal_middle_name, legal_last_name,
                personal_phone_e164, gender,
                citizen_id_country_code, citizen_id_last4,
                (legal_first_name IS NOT NULL AND legal_last_name IS NOT NULL) AS "profile_complete!",
                status, hire_date, termination_date, version, created_at, updated_at
            "#,
            tenant_id,
            employee_id,
            input.account_id,
            input.employee_code,
            input.display_name,
            input.legal_first_name,
            input.legal_middle_name,
            input.legal_last_name,
            input.personal_phone_e164,
            input.gender.map(|gender: Gender| gender.as_code()),
            input.status.as_code(),
            input.hire_date,
            input.termination_date,
            audit_account_id,
            expected_version,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| mutation_failure("update employee", tenant_id, err))?;

        let row: EmployeeRow = row.ok_or(PeopleOpsErr::Conflict)?;
        tran.commit()
            .await
            .map_err(|err: sqlx::Error| database_failure("commit employee update", tenant_id, err))?;
        info!(
            "Employee updated: tenant_id={} employee_id={} status={} audit_account_id={}",
            tenant_id,
            employee_id,
            input.status.as_code(),
            audit_account_id
        );
        Employee::try_from(row)
    }

    pub async fn find_employee_sensitive_profile(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeSensitiveProfile>, PeopleOpsErr> {
        let row: Option<EmployeeSensitiveRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                sqlx::query_as!(
                    EmployeeSensitiveRow,
                    r#"
                    SELECT id AS employee_id, citizen_id_country_code, citizen_id_key_id,
                           citizen_id_ciphertext, version
                    FROM hr_employees
                    WHERE tenant_id = $1 AND id = $2
                    "#,
                    tenant_id,
                    employee_id,
                )
                .fetch_optional(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("find sensitive employee profile", tenant_id, err))?;
        row.map(|sensitive_row: EmployeeSensitiveRow| self.sensitive_profile(tenant_id, sensitive_row))
            .transpose()
    }

    pub async fn find_employee_sensitive_profile_by_account(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
    ) -> Result<Option<EmployeeSensitiveProfile>, PeopleOpsErr> {
        let row: Option<EmployeeSensitiveRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                sqlx::query_as!(
                    EmployeeSensitiveRow,
                    r#"
                    SELECT id AS employee_id, citizen_id_country_code, citizen_id_key_id,
                           citizen_id_ciphertext, version
                    FROM hr_employees
                    WHERE tenant_id = $1 AND account_id = $2
                    "#,
                    tenant_id,
                    account_id,
                )
                .fetch_optional(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| {
                tenant_database_failure("find own sensitive employee profile", tenant_id, err)
            })?;
        row.map(|sensitive_row: EmployeeSensitiveRow| self.sensitive_profile(tenant_id, sensitive_row))
            .transpose()
    }

    pub async fn update_employee_citizen_id(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeCitizenIdInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeSensitiveProfile, PeopleOpsErr> {
        use crate::people::security::CitizenIdProtectErr;
        let protected: Option<ProtectedCitizenId> =
            match (input.citizen_id_country_code.as_deref(), input.citizen_id.as_deref()) {
                (Some(country_code), Some(citizen_id)) => Some(
                    self.citizen_id_protector
                        .protect(tenant_id, country_code, citizen_id)
                        .map_err(|err: CitizenIdProtectErr| {
                            error!(
                                tenant_id = %tenant_id,
                                employee_id = %employee_id,
                                reason = %err,
                                "Employee citizen ID could not be encrypted"
                            );
                            PeopleOpsErr::BackendUnavailable
                        })?,
                ),
                (None, None) => None,
                _ => return Err(PeopleOpsErr::InvalidInput("citizen ID input is inconsistent")),
            };

        let mut tran: TenantTransaction = self.begin_active_tenant(tenant_id).await?;
        let current: Option<EmployeeSensitiveUpdateRow> = sqlx::query_as!(
            EmployeeSensitiveUpdateRow,
            r#"
            SELECT branch_id, citizen_id_country_code, citizen_id_last4, version
            FROM hr_employees
            WHERE tenant_id = $1 AND id = $2
            FOR UPDATE
            "#,
            tenant_id,
            employee_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("lock employee citizen ID", tenant_id, err))?;

        let current: EmployeeSensitiveUpdateRow = current.ok_or(PeopleOpsErr::NotFound)?;
        if current.version != input.expected_version {
            return Err(PeopleOpsErr::Conflict);
        }
        let action: &str = match (current.citizen_id_last4.is_some(), protected.is_some()) {
            (false, true) => "set",
            (true, true) => "replace",
            (true, false) => "clear",
            (false, false) => "clear",
        };
        let new_country_code: Option<&str> = input.citizen_id_country_code.as_deref();
        let new_citizen_id: Option<&str> = input.citizen_id.as_deref();
        let new_key_id: Option<&str> = protected
            .as_ref()
            .map(|value: &ProtectedCitizenId| value.key_id.as_str());
        let new_ciphertext: Option<&[u8]> = protected
            .as_ref()
            .map(|value: &ProtectedCitizenId| value.ciphertext.as_slice());
        let new_lookup_hmac: Option<&[u8]> = protected
            .as_ref()
            .map(|value: &ProtectedCitizenId| value.lookup_hmac.as_slice());
        let new_last4: Option<&str> = protected
            .as_ref()
            .map(|value: &ProtectedCitizenId| value.last4.as_str());
        let version: i64 = sqlx::query_scalar!(
            r#"
            UPDATE hr_employees
            SET citizen_id_country_code = $3,
                citizen_id_key_id = $4,
                citizen_id_ciphertext = $5,
                citizen_id_lookup_hmac = $6,
                citizen_id_last4 = $7,
                version = version + 1,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $8
            WHERE tenant_id = $1 AND id = $2 AND version = $9
            RETURNING version
            "#,
            tenant_id,
            employee_id,
            new_country_code,
            new_key_id,
            new_ciphertext,
            new_lookup_hmac,
            new_last4,
            audit_account_id,
            input.expected_version,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| mutation_failure("update employee citizen ID", tenant_id, err))?
        .ok_or(PeopleOpsErr::Conflict)?;

        sqlx::query!(
            r#"
            INSERT INTO hr_employee_sensitive_audit_log (
                id, tenant_id, branch_id, employee_id, action,
                previous_country_code, previous_last4, new_country_code, new_last4,
                changed_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            Uuid::new_v4(),
            tenant_id,
            current.branch_id,
            employee_id,
            action,
            current.citizen_id_country_code,
            current.citizen_id_last4,
            new_country_code,
            new_last4,
            audit_account_id,
        )
        .execute(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("audit employee citizen ID update", tenant_id, err))?;
        tran.commit()
            .await
            .map_err(|err: sqlx::Error| database_failure("commit employee citizen ID update", tenant_id, err))?;
        info!(
            tenant_id = %tenant_id,
            employee_id = %employee_id,
            action,
            audit_account_id = %audit_account_id,
            "Employee citizen ID updated without logging credential material"
        );
        Ok(EmployeeSensitiveProfile {
            employee_id,
            citizen_id_country_code: input.citizen_id_country_code.clone(),
            citizen_id: new_citizen_id.map(str::to_owned),
            version,
        })
    }

    #[cfg(feature = "hrm-attendance")]
    pub async fn list_attendance_sessions(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        limit: i64,
        cursor: Option<&AttendanceCursor>,
    ) -> Result<AttendancePage, PeopleOpsErr> {
        let cursor_check_in_at: Option<DateTime<Utc>> = cursor.map(|value: &AttendanceCursor| value.check_in_at);
        let cursor_id: Option<Uuid> = cursor.map(|value: &AttendanceCursor| value.attendance_session_id);
        let query_limit: i64 = limit + 1;
        let result: (bool, Vec<AttendanceSessionRow>) = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                let employee_exists: bool = sqlx::query_scalar!(
                    r#"SELECT EXISTS (
                        SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2
                    ) AS "exists!""#,
                    tenant_id,
                    employee_id,
                )
                .fetch_one(&mut *conn)
                .await?;
                let rows: Vec<AttendanceSessionRow> = sqlx::query_as!(
                    AttendanceSessionRow,
                    r#"
                    SELECT id, employee_id, branch_id, check_in_at, check_out_at,
                           worked_seconds, created_at, updated_at
                    FROM hr_attendance_sessions
                    WHERE tenant_id = $1 AND employee_id = $2
                      AND ($3::TIMESTAMPTZ IS NULL OR (check_in_at, id) < ($3, $4::UUID))
                    ORDER BY check_in_at DESC, id DESC
                    LIMIT $5
                    "#,
                    tenant_id,
                    employee_id,
                    cursor_check_in_at,
                    cursor_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await?;
                Ok((employee_exists, rows))
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("list employee attendance sessions", tenant_id, err))?;
        let (employee_exists, mut rows): (bool, Vec<AttendanceSessionRow>) = result;
        if !employee_exists {
            return Err(PeopleOpsErr::NotFound);
        }
        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<AttendanceCursor> = if has_more {
            rows.last().map(|row: &AttendanceSessionRow| AttendanceCursor {
                check_in_at: row.check_in_at,
                attendance_session_id: row.id,
            })
        } else {
            None
        };
        let items: Vec<AttendanceSession> = rows.into_iter().map(AttendanceSession::from).collect();
        Ok(AttendancePage { items, next_cursor })
    }

    #[cfg(feature = "hrm-attendance")]
    pub async fn check_in(
        &self,
        tenant_id: Uuid,
        attendance_session_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
        branch_id: Uuid,
    ) -> Result<AttendanceSession, PeopleOpsErr> {
        let row: Option<AttendanceSessionRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                sqlx::query_as!(
                    AttendanceSessionRow,
                    r#"
                    INSERT INTO hr_attendance_sessions (
                        id, tenant_id, branch_id, employee_id, check_in_by_account_id
                    )
                    SELECT $1, $2, branch.id, employee.id, $4
                    FROM hr_employees AS employee
                    INNER JOIN branches AS branch
                        ON branch.tenant_id = employee.tenant_id
                       AND branch.id = $5
                       AND branch.status = 'active'
                    WHERE employee.tenant_id = $2
                      AND employee.id = $3
                      AND employee.account_id = $4
                      AND employee.status = 'active'
                    RETURNING id, employee_id, branch_id, check_in_at, check_out_at,
                              worked_seconds, created_at, updated_at
                    "#,
                    attendance_session_id,
                    tenant_id,
                    employee_id,
                    account_id,
                    branch_id,
                )
                .fetch_optional(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_mutation_failure("check in employee", tenant_id, err))?;
        let row: AttendanceSessionRow = row.ok_or(PeopleOpsErr::NotFound)?;
        info!(
            "Employee checked in: tenant_id={} employee_id={} attendance_session_id={} account_id={} branch_id={}",
            tenant_id, employee_id, attendance_session_id, account_id, branch_id
        );
        Ok(row.into())
    }

    #[cfg(feature = "hrm-attendance")]
    pub async fn check_out(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
    ) -> Result<AttendanceSession, PeopleOpsErr> {
        let row: Option<AttendanceSessionRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                sqlx::query_as!(
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
                    RETURNING attendance.id, attendance.employee_id, attendance.branch_id,
                              attendance.check_in_at, attendance.check_out_at, attendance.worked_seconds,
                              attendance.created_at, attendance.updated_at
                    "#,
                    tenant_id,
                    employee_id,
                    account_id,
                )
                .fetch_optional(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_mutation_failure("check out employee", tenant_id, err))?;
        let row: AttendanceSessionRow = row.ok_or(PeopleOpsErr::NotFound)?;
        info!(
            "Employee checked out: tenant_id={} employee_id={} attendance_session_id={} account_id={}",
            tenant_id, employee_id, row.id, account_id
        );
        Ok(row.into())
    }
}

fn database_failure(operation: &str, tenant_id: Uuid, err: sqlx::Error) -> PeopleOpsErr {
    error!(
        "HR db operation failed: operation={} tenant_id={} err={}",
        operation, tenant_id, err
    );
    PeopleOpsErr::BackendUnavailable
}

fn tenant_database_failure(op: &str, tenant_id: Uuid, err: TenantDbErr) -> PeopleOpsErr {
    error!(
        op,
        tenant_id = %tenant_id,
        reason = %err,
        "HR automatic tenant operation failed"
    );
    PeopleOpsErr::BackendUnavailable
}

fn tenant_mutation_failure(op: &str, tenant_id: Uuid, err: TenantDbErr) -> PeopleOpsErr {
    match err {
        TenantDbErr::Sqlx(sqlx_error) => mutation_failure(op, tenant_id, sqlx_error),
        tenant_error => tenant_database_failure(op, tenant_id, tenant_error),
    }
}

fn mutation_failure(op: &str, tenant_id: Uuid, err: sqlx::Error) -> PeopleOpsErr {
    let mapped: PeopleOpsErr = err.as_database_error().map_or(
        PeopleOpsErr::BackendUnavailable,
        |db_err: &dyn sqlx::error::DatabaseError| {
            if db_err.is_unique_violation() || db_err.code().as_deref() == Some("23505") {
                PeopleOpsErr::Conflict
            } else if db_err.is_foreign_key_violation() || db_err.is_check_violation() {
                PeopleOpsErr::InvalidInput("a referenced HR record is invalid")
            } else {
                PeopleOpsErr::BackendUnavailable
            }
        },
    );
    error!(
        "HR mutation failed: operation={} tenant_id={} mapped_error={:?} err={}",
        op, tenant_id, mapped, err
    );
    mapped
}
