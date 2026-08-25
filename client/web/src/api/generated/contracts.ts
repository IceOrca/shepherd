// This file is generated from Rust API DTOs. Do not edit it manually.

export type RoleCode = string;

export type PermissionCode = string;

export type AccountStatus = "active" | "disabled";

export type AuthProviderUserStatus = "active" | "disabled" | "missing";

export type CurrentUserProfile = { tenant_id: string, account_id: string, username: string, email: string | null, primary_role: RoleCode, roles: Array<RoleCode>, permissions: Array<PermissionCode>, branch_ids: Array<string>, active_branch_id: string | null, };

export type TenantMembershipSummary = { tenant_id: string, account_id: string, tenant_slug: string, tenant_display_name: string, username: string, email: string | null, primary_role: RoleCode, };

export type AuthUserSummary = { auth_user_id: string, account_id: string, username: string, email: string | null, primary_role: RoleCode, branch_ids: Array<string>, account_status: AccountStatus, provider_status: AuthProviderUserStatus, email_confirmed: boolean, created_at: string | null, last_sign_in_at: string | null, };

export type CreateAuthUserRequest = { username: string, email: string, password: string | null, primary_role: RoleCode, branch_ids: Array<string>, };

export type SetAuthUserStatusRequest = { disabled: boolean, };

export type AccessRoleScope = "tenant" | "branch";

export type PermissionOverrideEffect = "allow" | "deny";

export type AccessControlBranch = { id: string, code: string, name: string, time_zone: string, status: string, version: number, };

export type AccessControlPermission = { code: PermissionCode, display_name: string, description: string, };

export type AccessControlRole = { code: RoleCode, display_name: string, description: string | null, scope: AccessRoleScope, is_system: boolean, is_active: boolean, version: number, permission_codes: Array<PermissionCode>, assigned_account_count: number, };

export type AccountRoleAssignmentContract = { role_code: RoleCode, branch_id: string | null, };

export type AccountPermissionOverrideContract = { permission_code: PermissionCode, branch_id: string | null, effect: PermissionOverrideEffect, expires_at: string | null, };

export type AccessControlUser = { account_id: string, username: string, email: string | null, status: AccountStatus, primary_role: RoleCode, authorization_version: number, assignments: Array<AccountRoleAssignmentContract>, permission_overrides: Array<AccountPermissionOverrideContract>, };

export type AccessControlAuditEntry = { id: string, actor_account_id: string, action: string, object_type: string, object_id: string, branch_id: string | null, before_value: unknown, after_value: unknown, created_at: string, };

export type AccessControlSnapshot = { branches: Array<AccessControlBranch>, permissions: Array<AccessControlPermission>, roles: Array<AccessControlRole>, users: Array<AccessControlUser>, audit: Array<AccessControlAuditEntry>, };

export type CreateAccessControlBranchRequest = { code: string, name: string, time_zone: string, };

export type UpdateAccessControlBranchRequest = { name: string, time_zone: string, status: string, expected_version: number, };

export type CreateAccessControlRoleRequest = { code: RoleCode, display_name: string, description: string | null, scope: AccessRoleScope, permission_codes: Array<PermissionCode>, };

export type UpdateAccessControlRoleRequest = { display_name: string, description: string | null, is_active: boolean, expected_version: number, permission_codes: Array<PermissionCode>, };

export type UpdateAccountAccessRequest = { primary_role: RoleCode, expected_version: number, assignments: Array<AccountRoleAssignmentContract>, permission_overrides: Array<AccountPermissionOverrideContract>, };

export type BranchSummary = { id: string, code: string, name: string, time_zone: string, };

export type BusinessRecordStatus = "active" | "disabled";

export type StaffingShiftStatus = "open" | "filled" | "in_progress" | "completed" | "cancelled";

export type ShiftAssignmentStatus = "assigned" | "approved" | "cancelled";

export type RateSource = "configured" | "manual";

export type StaffingRateKind = "customer_bill" | "worker_pay";

export type Customer = { id: string, code: string, name: string, address: string | null, time_zone: string, billing_email: string | null, status: BusinessRecordStatus, created_at: string, updated_at: string, };

export type StaffingRate = { id: string, rate_kind: StaffingRateKind, code: string, name: string, customer_id: string | null, employee_id: string | null, job_id: string, currency: string, hourly_rate: string, priority: number, effective_from: string, effective_to: string | null, is_active: boolean, created_at: string, };

export type StaffingShift = { id: string, customer_id: string, job_id: string, starts_at: string, ends_at: string, required_workers: number, status: StaffingShiftStatus, notes: string | null, created_at: string, updated_at: string, };

export type ShiftAssignment = { id: string, shift_id: string, employee_id: string, customer_bill_rate_id: string | null, worker_pay_rate_id: string | null, rate_source: RateSource, manual_rate_reason: string | null, currency: string, bill_hourly_rate_snapshot: string, worker_hourly_rate_snapshot: string, eligibility_exception_reason: string | null, status: ShiftAssignmentStatus, worked_seconds: number | null, observed_worked_seconds: number | null, approval_adjustment_reason: string | null, customer_amount: string | null, worker_amount: string | null, margin_amount: string | null, approved_at: string | null, created_at: string, };

export type StaffingCandidate = { employee_id: string, employee_code: string, display_name: string, suitable: boolean, available: boolean, already_assigned: boolean, conflict_shift_id: string | null, };

export type StaffingEligibility = { id: string, employee_id: string, job_id: string, effective_from: string, effective_to: string | null, notes: string | null, created_at: string, };

export type ReconciliationStatus = "pending_staff" | "pending_customer" | "matched" | "discrepancy" | "reconciled";

export type CustomerWorkRecord = { id: string, assignment_id: string, confirmed_customer_id: string, confirmed_started_at: string, confirmed_ended_at: string, confirmed_worked_seconds: number, customer_reference: string | null, notes: string | null, updated_at: string, };

export type StaffingReconciliation = { assignment_id: string, shift_id: string, customer_id: string, employee_id: string, employee_code: string, employee_name: string, customer_name: string, scheduled_starts_at: string, scheduled_ends_at: string, assignment_status: ShiftAssignmentStatus, staff_started_at: string | null, staff_ended_at: string | null, staff_worked_seconds: number, customer_record: CustomerWorkRecord | null, final_worked_seconds: number | null, adjustment_reason: string | null, reconciliation_status: ReconciliationStatus, };

export type CustomerWorkRecordUpsertRequest = { confirmed_customer_id: string, confirmed_started_at: string, confirmed_ended_at: string, customer_reference?: string | null, notes?: string | null, };

export type CustomerUpsertRequest = { code: string, name: string, address?: string | null, time_zone: string, billing_email?: string | null, status: BusinessRecordStatus, };

export type StaffingRateCreateRequest = { rate_kind: StaffingRateKind, code: string, name: string, customer_id?: string | null, employee_id?: string | null, job_id: string, currency: string, hourly_rate: string, priority: number, effective_from: string, effective_to?: string | null, is_active: boolean, };

export type StaffingEligibilityCreateRequest = { employee_id: string, job_id: string, effective_from: string, effective_to?: string | null, notes?: string | null, };

export type StaffingShiftCreateRequest = { customer_id: string, job_id: string, starts_at: string, ends_at: string, required_workers: number, notes?: string | null, };

export type ManualRateOverrideRequest = { reason: string, currency: string, bill_hourly_rate: string, worker_hourly_rate: string, };

export type ShiftAssignmentCreateRequest = { employee_id: string, manual_rate?: ManualRateOverrideRequest | null, };

export type ShiftAssignmentApproveRequest = { worked_seconds?: number | null, adjustment_reason?: string | null, };

export type ShiftWorkSession = { id: string, assignment_id: string, employee_id: string, started_at: string, ended_at: string | null, worked_seconds: number | null, started_latitude: number | null, started_longitude: number | null, started_accuracy_meters: number | null, ended_latitude: number | null, ended_longitude: number | null, ended_accuracy_meters: number | null, created_at: string, updated_at: string, };

export type OwnStaffingAssignment = { assignment_id: string, shift_id: string, customer_name: string, starts_at: string, ends_at: string, status: ShiftAssignmentStatus, observed_worked_seconds: number, is_working: boolean, staff_started_at: string | null, staff_ended_at: string | null, };

export type ShiftWorkActionRequest = { latitude?: number | null, longitude?: number | null, accuracy_meters?: number | null, };

export type UrgentWorkStatus = "active" | "completed" | "reconciled" | "cancelled";

export type UrgentWorkActionSource = "self_reported" | "peer";

export type UrgentWorkCustomer = { customer_id: string, customer_name: string, address: string | null, time_zone: string, };

export type UrgentWorkEmployee = { employee_id: string, employee_code: string, display_name: string, is_self: boolean, has_open_work: boolean, };

export type UrgentWorkItem = { report_id: string, branch_id: string, branch_name: string, employee_id: string, employee_code: string, employee_name: string, claimed_customer_id: string, customer_name: string, status: UrgentWorkStatus, started_at: string, ended_at: string | null, worked_seconds: number | null, started_by_account_id: string, started_by_username: string, start_source: UrgentWorkActionSource, ended_by_account_id: string | null, ended_by_username: string | null, end_source: UrgentWorkActionSource | null, reconciled_assignment_id: string | null, created_at: string, updated_at: string, };

export type UrgentCustomerWorkRecord = { id: string, report_id: string, confirmed_customer_id: string, confirmed_customer_name: string, confirmed_started_at: string, confirmed_ended_at: string, confirmed_worked_seconds: number, customer_reference: string | null, notes: string | null, updated_at: string, };

export type UrgentWorkReconciliation = { work: UrgentWorkItem, customer_record: UrgentCustomerWorkRecord | null, reconciliation_status: ReconciliationStatus, final_customer_id: string | null, final_job_id: string | null, final_worked_seconds: number | null, adjustment_reason: string | null, eligibility_exception_reason: string | null, };

export type UrgentWorkStartRequest = { customer_id: string, employee_ids: Array<string>, latitude: number | null, longitude: number | null, accuracy_meters: number | null, };

export type UrgentWorkEndRequest = { latitude: number | null, longitude: number | null, accuracy_meters: number | null, };

export type UrgentCustomerWorkRecordUpsertRequest = { confirmed_customer_id: string, confirmed_started_at: string, confirmed_ended_at: string, customer_reference?: string | null, notes?: string | null, };

export type UrgentWorkReconcileRequest = { final_customer_id: string, job_id: string, worked_seconds: number, adjustment_reason?: string | null, eligibility_exception_reason?: string | null, manual_rate?: ManualRateOverrideRequest | null, };

export type PayBasis = "hourly" | "monthly";

export type PayrollRunStatus = "draft" | "calculated" | "approved" | "paid";

export type EmployeeCompensation = { id: string, employee_id: string, currency: string, pay_basis: PayBasis, hourly_rate: string | null, monthly_rate: string | null, standard_monthly_hours: string | null, effective_from: string, effective_to: string | null, created_at: string, };

export type BranchRateRule = { id: string, code: string, name: string, branch_id: string, employee_id: string | null, base_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to: string | null, is_active: boolean, };

export type TimeBandRule = { id: string, code: string, name: string, weekdays: Array<number>, start_time: string, end_time: string, spans_next_day: boolean, premium_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to: string | null, is_active: boolean, };

export type OvertimeRule = { id: string, code: string, name: string, threshold_minutes: number, premium_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to: string | null, is_active: boolean, };

export type PayrollEmployeeResult = { employee_id: string, worked_seconds: number, base_amount: string, branch_amount: string, time_amount: string, overtime_amount: string, gross_amount: string, currency: string, };

export type PayrollLine = { id: string, employee_id: string, attendance_session_id: string | null, staffing_assignment_id: string | null, branch_id: string, work_date: string, component: string, rule_code: string | null, worked_seconds: number, base_hourly_rate: string, multiplier: string, hourly_adjustment: string, amount: string, description: string, };

export type PayrollRun = { id: string, period_start: string, period_end: string, time_zone: string, currency: string, status: PayrollRunStatus, calculated_at: string | null, approved_at: string | null, created_at: string, results: Array<PayrollEmployeeResult>, lines: Array<PayrollLine>, };

export type EmployeeCompensationCreateRequest = { currency: string, pay_basis: PayBasis, hourly_rate?: string | null, monthly_rate?: string | null, standard_monthly_hours?: string | null, effective_from: string, effective_to?: string | null, };

export type BranchRateRuleCreateRequest = { code: string, name: string, branch_id: string, employee_id?: string | null, base_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to?: string | null, is_active: boolean, };

export type TimeBandRuleCreateRequest = { code: string, name: string, weekdays: Array<number>, start_time: string, end_time: string, spans_next_day: boolean, premium_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to?: string | null, is_active: boolean, };

export type OvertimeRuleCreateRequest = { code: string, name: string, threshold_minutes: number, premium_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to?: string | null, is_active: boolean, };

export type PayrollCalculateRequest = { year: number, month: number, time_zone: string, currency: string, };

export type EmployeeStatus = "active" | "on_leave" | "terminated";

export type HrRecordStatus = "active" | "archived";

export type Employee = { id: string, account_id: string | null, employee_code: string, display_name: string, work_email: string | null, work_phone: string | null, badge_id: string | null, status: EmployeeStatus, hire_date: string, termination_date: string | null, created_at: string, updated_at: string, };

export type Department = { id: string, code: string, name: string, parent_department_id: string | null, manager_employee_id: string | null, status: HrRecordStatus, created_at: string, updated_at: string, };

export type JobPosition = { id: string, code: string, name: string, department_id: string | null, status: HrRecordStatus, created_at: string, updated_at: string, };

export type EmployeeAssignment = { id: string, employee_id: string, branch_id: string, department_id: string | null, job_id: string | null, manager_employee_id: string | null, date_start: string, date_end: string | null, is_primary: boolean, created_at: string, };

export type AttendanceSession = { id: string, employee_id: string, branch_id: string, check_in_at: string, check_out_at: string | null, worked_seconds: number | null, created_at: string, updated_at: string, };

export type AttendanceCheckInRequest = { branch_id: string, };

export type EmployeeUpsertRequest = { account_id?: string | null, employee_code: string, display_name: string, work_email?: string | null, work_phone?: string | null, badge_id?: string | null, status: EmployeeStatus, hire_date: string, termination_date?: string | null, };

export type DepartmentUpsertRequest = { code: string, name: string, parent_department_id?: string | null, manager_employee_id?: string | null, status: HrRecordStatus, };

export type JobPositionUpsertRequest = { code: string, name: string, department_id?: string | null, status: HrRecordStatus, };

export type EmployeeAssignmentCreateRequest = { branch_id: string, department_id?: string | null, job_id?: string | null, manager_employee_id?: string | null, date_start: string, date_end?: string | null, is_primary: boolean, };

export type WorkingPeriod = { id: string, weekday: number, start_time: string, end_time: string, spans_next_day: boolean, unpaid_break_minutes: number, };

export type WorkingSchedule = { id: string, code: string, name: string, time_zone: string, status: HrRecordStatus, periods: Array<WorkingPeriod>, created_at: string, updated_at: string, };

export type EmployeeScheduleAssignment = { id: string, employee_id: string, schedule_id: string, date_start: string, date_end: string | null, created_at: string, };

export type WorkingPeriodRequest = { weekday: number, start_time: string, end_time: string, spans_next_day: boolean, unpaid_break_minutes: number, };

export type EmployeeScheduleAssignmentView = { assignment: EmployeeScheduleAssignment, schedule: WorkingSchedule, };

export type WorkingScheduleUpsertRequest = { code: string, name: string, time_zone: string, status: HrRecordStatus, periods: Array<WorkingPeriodRequest>, };

export type EmployeeScheduleAssignmentCreateRequest = { schedule_id: string, date_start: string, date_end?: string | null, };

