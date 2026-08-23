use std::{
    error::Error,
    io,
    sync::{Arc, Once},
};

use infra_postgres::DatabaseAdapter;
use uuid::Uuid;

use super::{
    core::{ShiftWorkActionInput, ShiftWorkSession, StaffingWorkRepo},
    database::StaffingWorkDb,
};
use crate::business::staffing::{
    core::{ShiftAssignmentStatus, StaffingError, StaffingRepo},
    database::StaffingDb,
};

type TestResult = Result<(), Box<dyn Error>>;

struct Fixture {
    db: Arc<DatabaseAdapter>,
    tenant_id: Uuid,
    branch_id: Uuid,
    account_id: Uuid,
    assignment_id: Uuid,
}

impl Fixture {
    async fn create() -> Result<Self, Box<dyn Error>> {
        init_tracing();
        let database_url = std::env::var("DATABASE_URL")?;
        let db = DatabaseAdapter::connect(&database_url).await?;
        let tenant_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let employee_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let customer_id = Uuid::new_v4();
        let shift_id = Uuid::new_v4();
        let assignment_id = Uuid::new_v4();
        let tenant_slug = format!("staffing-work-cases-{}", tenant_id.simple());

        db.provision_tenant(tenant_id, &tenant_slug, "Staffing work cases tenant")
            .await?;
        let mut setup = db.begin_tenant(tenant_id).await?;
        sqlx::query!(
            r#"
            INSERT INTO accounts (id, tenant_id, username, primary_role_code)
            VALUES ($1, $2, 'staffing-work-cases', 'staff')
            "#,
            account_id,
            tenant_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO account_roles (tenant_id, account_id, role_code)
            VALUES ($1, $2, 'staff')
            "#,
            tenant_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO branches (id, tenant_id, code, name, time_zone)
            VALUES ($1, $2, 'staffing-work-branch', 'Staffing Work Branch', 'Asia/Bangkok')
            "#,
            branch_id,
            tenant_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, branch_id, account_id, employee_code, display_name, status, hire_date
            )
            VALUES ($1, $2, $3, $4, 'staffing-work-cases', 'Staffing Work Cases', 'active', CURRENT_DATE)
            "#,
            employee_id,
            tenant_id,
            branch_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO hr_jobs (id, tenant_id, branch_id, code, name, status)
            VALUES ($1, $2, $3, 'staffing-work-cases', 'Staffing Work Cases', 'active')
            "#,
            job_id,
            tenant_id,
            branch_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_customers (
                id, tenant_id, branch_id, code, name, address, time_zone,
                created_by_account_id, updated_by_account_id
            )
            VALUES (
                $1, $2, $3, 'staffing-work-cases', 'Staffing Work Customer',
                'Staffing work address', 'Asia/Bangkok', $4, $4
            )
            "#,
            customer_id,
            tenant_id,
            branch_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_staffing_shifts (
                id, tenant_id, branch_id, customer_id, job_id,
                starts_at, ends_at, required_workers, created_by_account_id, updated_by_account_id
            )
            VALUES (
                $1, $2, $3, $4, $5, CURRENT_TIMESTAMP - INTERVAL '1 hour',
                CURRENT_TIMESTAMP + INTERVAL '8 hours', 1, $6, $6
            )
            "#,
            shift_id,
            tenant_id,
            branch_id,
            customer_id,
            job_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_shift_assignments (
                id, tenant_id, branch_id, shift_id, employee_id, rate_source, manual_rate_reason, currency,
                bill_hourly_rate_snapshot, worker_hourly_rate_snapshot, created_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, 'manual', 'isolated staffing test rate', 'VND', 150000, 120000, $6)
            "#,
            assignment_id,
            tenant_id,
            branch_id,
            shift_id,
            employee_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO notification_destinations (id, tenant_id, branch_id, channel, destination)
            VALUES ($1, $2, $3, 'telegram', '-1000000000001')
            "#,
            Uuid::new_v4(),
            tenant_id,
            branch_id,
        )
        .execute(setup.connection())
        .await?;
        setup.commit().await?;

        Ok(Self {
            db,
            tenant_id,
            branch_id,
            account_id,
            assignment_id,
        })
    }

    fn work_provider(&self) -> Arc<StaffingWorkDb> {
        StaffingWorkDb::new_arc(Arc::clone(&self.db))
    }

    fn staffing_provider(&self) -> Arc<StaffingDb> {
        StaffingDb::new_arc(Arc::clone(&self.db))
    }

    async fn age_session(&self, session_id: Uuid, seconds: f64) -> Result<(), Box<dyn Error>> {
        let mut transaction = self.db.begin_tenant(self.tenant_id).await?;
        sqlx::query!(
            r#"
            UPDATE business_shift_work_sessions
            SET started_at = CURRENT_TIMESTAMP - make_interval(secs => $3)
            WHERE tenant_id = $1 AND id = $2
            "#,
            self.tenant_id,
            session_id,
            seconds,
        )
        .execute(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn disable_destinations(&self) -> Result<(), Box<dyn Error>> {
        let mut transaction = self.db.begin_tenant(self.tenant_id).await?;
        sqlx::query!(
            "UPDATE notification_destinations SET enabled = FALSE WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn outbox_count(&self) -> Result<i64, Box<dyn Error>> {
        let mut transaction = self.db.begin_tenant(self.tenant_id).await?;
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM notification_outbox WHERE tenant_id = $1"#,
            self.tenant_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(count)
    }

    async fn session_count(&self) -> Result<i64, Box<dyn Error>> {
        let mut transaction = self.db.begin_tenant(self.tenant_id).await?;
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM business_shift_work_sessions WHERE tenant_id = $1"#,
            self.tenant_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(count)
    }

    async fn pending_outbox_count(&self) -> Result<i64, Box<dyn Error>> {
        let mut transaction = self.db.begin_tenant(self.tenant_id).await?;
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM notification_outbox
            WHERE tenant_id = $1
              AND status = 'pending'
              AND attempt_count = 0
              AND locked_at IS NULL
              AND sent_at IS NULL
            "#,
            self.tenant_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(count)
    }

    async fn financial_snapshot_is_consistent(&self) -> Result<bool, Box<dyn Error>> {
        let mut transaction = self.db.begin_tenant(self.tenant_id).await?;
        let consistent = sqlx::query_scalar!(
            r#"
            SELECT (
                worked_seconds = observed_worked_seconds
                AND customer_amount >= worker_amount
                AND margin_amount = customer_amount - worker_amount
            ) AS "consistent!"
            FROM business_shift_assignments
            WHERE tenant_id = $1 AND id = $2
            "#,
            self.tenant_id,
            self.assignment_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(consistent)
    }

    async fn record_matching_customer_evidence(&self) -> Result<(), Box<dyn Error>> {
        let mut transaction: infra_postgres::TenantTransaction = self.db.begin_tenant(self.tenant_id).await?;
        sqlx::query!(
            r#"
            INSERT INTO business_customer_work_records (
                id, tenant_id, branch_id, assignment_id, confirmed_customer_id,
                confirmed_started_at, confirmed_ended_at, customer_reference,
                recorded_by_account_id
            )
            SELECT $1, $2, assignment.branch_id, $3, shift.customer_id,
                   observed.started_at, observed.ended_at,
                   'test-customer-record', $4
            FROM business_shift_assignments AS assignment
            INNER JOIN business_staffing_shifts AS shift
                ON shift.tenant_id = assignment.tenant_id
               AND shift.id = assignment.shift_id
            CROSS JOIN LATERAL (
                SELECT MIN(started_at) FILTER (WHERE ended_at IS NOT NULL) AS started_at,
                       MAX(ended_at) AS ended_at,
                       COALESCE(SUM(worked_seconds), 0)::BIGINT AS total
                FROM business_shift_work_sessions
                WHERE tenant_id = $2 AND assignment_id = $3 AND ended_at IS NOT NULL
            ) AS observed
            WHERE assignment.tenant_id = $2
              AND assignment.id = $3
              AND observed.total > 0
            "#,
            Uuid::new_v4(),
            self.tenant_id,
            self.assignment_id,
            self.account_id,
        )
        .execute(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn cleanup(self) -> Result<(), Box<dyn Error>> {
        let mut transaction = self.db.begin_tenant(self.tenant_id).await?;
        sqlx::query!("DELETE FROM notification_outbox WHERE tenant_id = $1", self.tenant_id)
            .execute(transaction.connection())
            .await?;
        sqlx::query!(
            "DELETE FROM notification_destinations WHERE tenant_id = $1",
            self.tenant_id
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!(
            "DELETE FROM business_shift_work_sessions WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!(
            "DELETE FROM business_customer_work_records WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!(
            "DELETE FROM business_shift_assignments WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!(
            "DELETE FROM business_staffing_shifts WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query!("DELETE FROM business_customers WHERE tenant_id = $1", self.tenant_id)
            .execute(transaction.connection())
            .await?;
        sqlx::query!("DELETE FROM hr_employees WHERE tenant_id = $1", self.tenant_id)
            .execute(transaction.connection())
            .await?;
        sqlx::query!("DELETE FROM hr_jobs WHERE tenant_id = $1", self.tenant_id)
            .execute(transaction.connection())
            .await?;
        sqlx::query!("DELETE FROM accounts WHERE tenant_id = $1", self.tenant_id)
            .execute(transaction.connection())
            .await?;
        sqlx::query!("DELETE FROM branches WHERE tenant_id = $1", self.tenant_id)
            .execute(transaction.connection())
            .await?;
        transaction.commit().await?;
        sqlx::query!("DELETE FROM tenants WHERE id = $1", self.tenant_id)
            .execute(self.db.global_pool())
            .await?;
        Ok(())
    }
}

fn init_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let result: Result<(), Box<dyn Error + Send + Sync>> = tracing_subscriber::fmt()
            .with_env_filter("shepherd=trace,infra_postgres=debug")
            .with_test_writer()
            .try_init();
        let _ignored_already_initialized: Result<(), Box<dyn Error + Send + Sync>> = result;
    });
}

fn action_input() -> ShiftWorkActionInput {
    ShiftWorkActionInput {
        idempotency_key: Uuid::new_v4(),
        latitude: None,
        longitude: None,
        accuracy_meters: None,
    }
}

fn located_input(latitude: f64, longitude: f64, accuracy_meters: f32) -> ShiftWorkActionInput {
    ShiftWorkActionInput {
        idempotency_key: Uuid::new_v4(),
        latitude: Some(latitude),
        longitude: Some(longitude),
        accuracy_meters: Some(accuracy_meters),
    }
}

fn staffing_error(error: StaffingError) -> io::Error {
    io::Error::other(format!("staffing operation failed: {error:?}"))
}

fn one_success_one_conflict(
    left: Result<ShiftWorkSession, StaffingError>,
    right: Result<ShiftWorkSession, StaffingError>,
) -> Result<(ShiftWorkSession, bool), io::Error> {
    match (left, right) {
        (Ok(session), Err(StaffingError::Conflict)) => Ok((session, true)),
        (Err(StaffingError::Conflict), Ok(session)) => Ok((session, false)),
        (left, right) => Err(io::Error::other(format!(
            "expected one success and one conflict, got {left:?} and {right:?}"
        ))),
    }
}

#[tokio::test]
async fn regular_flow_supports_multiple_sessions_and_durable_outbox_events() -> TestResult {
    let fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let work = fixture.work_provider();
        let staffing = fixture.staffing_provider();

        let initial = work
            .list_own_assignments(fixture.tenant_id, fixture.account_id)
            .await
            .map_err(staffing_error)?;
        assert_eq!(initial.len(), 1);
        let initial_assignment = initial
            .first()
            .ok_or_else(|| io::Error::other("own assignment was not returned"))?;
        assert!(!initial_assignment.is_working);
        assert_eq!(initial_assignment.observed_worked_seconds, 0);

        let start_input = located_input(10.7769, 106.7009, 8.5);
        let first = work
            .start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                &start_input,
            )
            .await
            .map_err(staffing_error)?;
        let repeated_start = work
            .start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                &start_input,
            )
            .await
            .map_err(staffing_error)?;
        assert_eq!(first.id, repeated_start.id);
        assert_eq!(first.started_latitude, start_input.latitude);
        assert_eq!(first.started_longitude, start_input.longitude);
        assert_eq!(first.started_accuracy_meters, start_input.accuracy_meters);

        fixture.age_session(first.id, 3600.0).await?;
        let end_input = located_input(10.7770, 106.7010, 6.0);
        let first_ended = work
            .end(fixture.tenant_id, fixture.assignment_id, fixture.account_id, &end_input)
            .await
            .map_err(staffing_error)?;
        let repeated_end = work
            .end(fixture.tenant_id, fixture.assignment_id, fixture.account_id, &end_input)
            .await
            .map_err(staffing_error)?;
        assert_eq!(first_ended.id, repeated_end.id);
        assert!(first_ended.worked_seconds.is_some_and(|seconds| seconds >= 3600));
        assert_eq!(first_ended.ended_latitude, end_input.latitude);
        assert_eq!(first_ended.ended_longitude, end_input.longitude);
        assert_eq!(first_ended.ended_accuracy_meters, end_input.accuracy_meters);

        let second_start_input = action_input();
        let second = work
            .start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                &second_start_input,
            )
            .await
            .map_err(staffing_error)?;
        fixture.age_session(second.id, 1800.0).await?;
        let second_ended = work
            .end(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                &action_input(),
            )
            .await
            .map_err(staffing_error)?;
        assert!(second_ended.worked_seconds.is_some_and(|seconds| seconds >= 1800));

        let completed = work
            .list_own_assignments(fixture.tenant_id, fixture.account_id)
            .await
            .map_err(staffing_error)?;
        let completed_assignment = completed
            .first()
            .ok_or_else(|| io::Error::other("completed assignment was not returned"))?;
        assert!(!completed_assignment.is_working);
        assert!(completed_assignment.observed_worked_seconds >= 5400);

        assert_eq!(fixture.session_count().await?, 2);
        assert_eq!(fixture.outbox_count().await?, 4);
        assert_eq!(fixture.pending_outbox_count().await?, 4);

        fixture.record_matching_customer_evidence().await?;

        let approved = staffing
            .approve_shift_assignment(
                fixture.tenant_id,
                fixture.assignment_id,
                None,
                Some("multiple staff sessions reconciled to one customer interval".to_owned()),
                fixture.account_id,
            )
            .await
            .map_err(staffing_error)?;
        assert_eq!(approved.status, ShiftAssignmentStatus::Approved);
        assert_eq!(approved.worked_seconds, approved.observed_worked_seconds);
        assert!(fixture.financial_snapshot_is_consistent().await?);

        let start_after_approval = work
            .start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                &action_input(),
            )
            .await;
        assert!(matches!(start_after_approval, Err(StaffingError::Conflict)));

        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn invalid_state_ownership_and_approval_transitions_are_rejected() -> TestResult {
    let fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let work = fixture.work_provider();
        let staffing = fixture.staffing_provider();

        let premature_end = work
            .end(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                &action_input(),
            )
            .await;
        assert!(matches!(premature_end, Err(StaffingError::Conflict)));

        let approval_without_work = staffing
            .approve_shift_assignment(fixture.tenant_id, fixture.assignment_id, None, None, fixture.account_id)
            .await;
        assert!(matches!(approval_without_work, Err(StaffingError::Conflict)));

        let wrong_account = Uuid::new_v4();
        let another_account_start = work
            .start(
                fixture.tenant_id,
                fixture.assignment_id,
                wrong_account,
                Uuid::new_v4(),
                &action_input(),
            )
            .await;
        assert!(matches!(another_account_start, Err(StaffingError::NotFound)));
        let another_account_assignments = work
            .list_own_assignments(fixture.tenant_id, wrong_account)
            .await
            .map_err(staffing_error)?;
        assert!(another_account_assignments.is_empty());

        let started = work
            .start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                &action_input(),
            )
            .await
            .map_err(staffing_error)?;

        let approval_while_open = staffing
            .approve_shift_assignment(fixture.tenant_id, fixture.assignment_id, None, None, fixture.account_id)
            .await;
        assert!(matches!(approval_while_open, Err(StaffingError::Conflict)));

        let overlapping_start = work
            .start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                &action_input(),
            )
            .await;
        assert!(matches!(overlapping_start, Err(StaffingError::Conflict)));

        fixture.age_session(started.id, 60.0).await?;
        work.end(
            fixture.tenant_id,
            fixture.assignment_id,
            fixture.account_id,
            &action_input(),
        )
        .await
        .map_err(staffing_error)?;

        let repeated_end_with_new_key = work
            .end(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                &action_input(),
            )
            .await;
        assert!(matches!(repeated_end_with_new_key, Err(StaffingError::Conflict)));

        fixture.record_matching_customer_evidence().await?;

        let override_without_reason = staffing
            .approve_shift_assignment(
                fixture.tenant_id,
                fixture.assignment_id,
                Some(120),
                None,
                fixture.account_id,
            )
            .await;
        assert!(matches!(override_without_reason, Err(StaffingError::Conflict)));

        let approved = staffing
            .approve_shift_assignment(
                fixture.tenant_id,
                fixture.assignment_id,
                Some(120),
                Some("Customer confirmed setup and cleanup time".to_owned()),
                fixture.account_id,
            )
            .await
            .map_err(staffing_error)?;
        assert_eq!(approved.worked_seconds, Some(120));
        assert!(approved.observed_worked_seconds.is_some_and(|seconds| seconds >= 60));
        assert_eq!(
            approved.approval_adjustment_reason.as_deref(),
            Some("Customer confirmed setup and cleanup time")
        );

        let repeated_approval = staffing
            .approve_shift_assignment(fixture.tenant_id, fixture.assignment_id, None, None, fixture.account_id)
            .await;
        assert!(matches!(repeated_approval, Err(StaffingError::Conflict)));

        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn concurrent_actions_create_exactly_one_session_transition() -> TestResult {
    let fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let work = fixture.work_provider();
        let start_left = action_input();
        let start_right = action_input();

        let (left_result, right_result) = tokio::join!(
            work.start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                &start_left,
            ),
            work.start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                &start_right,
            ),
        );
        let (started, left_started) = one_success_one_conflict(left_result, right_result)?;
        let winning_start = if left_started { &start_left } else { &start_right };
        let repeated_start = work
            .start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                winning_start,
            )
            .await
            .map_err(staffing_error)?;
        assert_eq!(started.id, repeated_start.id);

        fixture.age_session(started.id, 60.0).await?;
        let end_left = action_input();
        let end_right = action_input();
        let (left_result, right_result) = tokio::join!(
            work.end(fixture.tenant_id, fixture.assignment_id, fixture.account_id, &end_left,),
            work.end(fixture.tenant_id, fixture.assignment_id, fixture.account_id, &end_right,),
        );
        let (ended, left_ended) = one_success_one_conflict(left_result, right_result)?;
        let winning_end = if left_ended { &end_left } else { &end_right };
        let repeated_end = work
            .end(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                winning_end,
            )
            .await
            .map_err(staffing_error)?;
        assert_eq!(ended.id, repeated_end.id);

        assert_eq!(fixture.session_count().await?, 1);
        assert_eq!(fixture.outbox_count().await?, 2);
        assert_eq!(fixture.pending_outbox_count().await?, 2);

        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn missing_notification_destination_never_rolls_back_work() -> TestResult {
    let fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        fixture.disable_destinations().await?;
        let work = fixture.work_provider();

        let started = work
            .start(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                Uuid::new_v4(),
                &action_input(),
            )
            .await
            .map_err(staffing_error)?;
        fixture.age_session(started.id, 60.0).await?;
        let ended = work
            .end(
                fixture.tenant_id,
                fixture.assignment_id,
                fixture.account_id,
                &action_input(),
            )
            .await
            .map_err(staffing_error)?;

        assert!(ended.worked_seconds.is_some_and(|seconds| seconds >= 60));
        assert_eq!(fixture.session_count().await?, 1);
        assert_eq!(fixture.outbox_count().await?, 0);

        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}
