// This file is generated from Rust API DTOs. Do not edit it manually.

export type Role = "tenant_owner" | "supervisor" | "employee";

export type AccountStatus = "active" | "locked" | "disabled";

export type PermissionEffect = "allow" | "deny";

export type AccountPermission = { code: string, effect: PermissionEffect, expires_at: string | null, };

export type AccountSummary = { id: string, username: string, status: AccountStatus, 
/**
 * Built-in role used for authentication policy such as JWT lifetime.
 */
primary_role: Role, 
/**
 * Every active role assigned to the account, including custom roles.
 */
roles: Array<string>, auth_version: number, password_changed_at: string, last_authenticated_at: string | null, created_at: string, updated_at: string, };

export type RoleSummary = { code: string, display_name: string, description: string | null, is_system: boolean, is_active: boolean, permissions: Array<string>, };

export type PermissionSummary = { code: string, description: string, };

export type AuthorizationCatalog = { roles: Array<RoleSummary>, permissions: Array<PermissionSummary>, };

export type AuthRequest = { 
/**
 * Human-readable workspace slug from the platform tenant registry.
 */
tenant: string, username: string, passphrase: string, };

export type AuthResponse = { access_token: string, token_type: string, expires_in: number, };

export type AccessClaims = { 
/**
 * Account UUID from the shared accounts table.
 */
sub: string, 
/**
 * Tenant UUID from the shared tenants table.
 */
tid: string, iss: string, aud: string, exp: number, nbf: number, iat: number, jti: string, sid: string, username: string, role: Role, roles: Array<string>, 
/**
 * Account authorization version at token issuance.
 */
ver: number, permissions: Array<string>, };

export type RegisterUserRequest = { username: string, passphrase: string, role: Role, };

export type MessageResponse = { msg: string, };

export type AuthProfileResponse = { tenant_id: string, account_id: string, username: string, role: Role, roles: Array<string>, auth_version: number, permissions: Array<string>, };

export type UpdateAccountStatusRequest = { status: AccountStatus, };

export type UpdateAccountRolesRequest = { primary_role: Role, roles: Array<string>, };

export type UpdateAccountPermissionsRequest = { permissions: Array<AccountPermission>, };

export type ChangePasswordRequest = { current_passphrase: string, new_passphrase: string, };

export type ResetPasswordRequest = { new_passphrase: string, };

export type InvalidCredentialsResponse = { error: string, remaining_attempts: number, };

export type BranchSummary = { id: string, code: string, name: string, time_zone: string, };

export type FacilitySummary = { id: string, branch_id: string, code: string, name: string, };

export type BusinessRecordStatus = "active" | "disabled";

export type StaffingShiftStatus = "open" | "filled" | "in_progress" | "completed" | "cancelled";

export type ShiftAssignmentStatus = "assigned" | "approved" | "cancelled";

export type RateSource = "agreement" | "manual";

export type Customer = { id: string, code: string, name: string, billing_email: string | null, status: BusinessRecordStatus, created_at: string, updated_at: string, };

export type CustomerFacility = { id: string, customer_id: string, code: string, name: string, address: string | null, time_zone: string, status: BusinessRecordStatus, created_at: string, updated_at: string, };

export type StaffingRateAgreement = { id: string, code: string, name: string, customer_id: string, customer_facility_id: string | null, employee_id: string | null, job_id: string, currency: string, bill_hourly_rate: string, worker_hourly_rate: string, priority: number, effective_from: string, effective_to: string | null, is_active: boolean, created_at: string, };

export type StaffingShift = { id: string, customer_id: string, customer_facility_id: string, job_id: string, starts_at: string, ends_at: string, required_workers: number, status: StaffingShiftStatus, notes: string | null, created_at: string, updated_at: string, };

export type ShiftAssignment = { id: string, shift_id: string, employee_id: string, rate_agreement_id: string | null, rate_source: RateSource, currency: string, bill_hourly_rate_snapshot: string, worker_hourly_rate_snapshot: string, status: ShiftAssignmentStatus, worked_seconds: number | null, customer_amount: string | null, worker_amount: string | null, margin_amount: string | null, approved_at: string | null, created_at: string, };

export type CustomerCreateRequest = { code: string, name: string, billing_email?: string | null, status: BusinessRecordStatus, };

export type CustomerFacilityCreateRequest = { code: string, name: string, address?: string | null, time_zone: string, status: BusinessRecordStatus, };

export type StaffingRateAgreementCreateRequest = { code: string, name: string, customer_id: string, customer_facility_id?: string | null, employee_id?: string | null, job_id: string, currency: string, bill_hourly_rate: string, worker_hourly_rate: string, priority: number, effective_from: string, effective_to?: string | null, is_active: boolean, };

export type StaffingShiftCreateRequest = { customer_id: string, customer_facility_id: string, job_id: string, starts_at: string, ends_at: string, required_workers: number, notes?: string | null, };

export type ManualRateOverrideRequest = { currency: string, bill_hourly_rate: string, worker_hourly_rate: string, };

export type ShiftAssignmentCreateRequest = { employee_id: string, manual_rate?: ManualRateOverrideRequest | null, };

export type ShiftAssignmentApproveRequest = { worked_seconds: number, };

export type PayBasis = "hourly" | "monthly";

export type PayrollRunStatus = "draft" | "calculated" | "approved" | "paid";

export type EmployeeCompensation = { id: string, employee_id: string, currency: string, pay_basis: PayBasis, hourly_rate: string | null, monthly_rate: string | null, standard_monthly_hours: string | null, effective_from: string, effective_to: string | null, created_at: string, };

export type FacilityRateRule = { id: string, code: string, name: string, facility_id: string, employee_id: string | null, base_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to: string | null, is_active: boolean, };

export type TimeBandRule = { id: string, code: string, name: string, weekdays: Array<number>, start_time: string, end_time: string, spans_next_day: boolean, premium_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to: string | null, is_active: boolean, };

export type OvertimeRule = { id: string, code: string, name: string, threshold_minutes: number, premium_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to: string | null, is_active: boolean, };

export type PayrollEmployeeResult = { employee_id: string, worked_seconds: number, base_amount: string, facility_amount: string, time_amount: string, overtime_amount: string, gross_amount: string, currency: string, };

export type PayrollLine = { id: string, employee_id: string, attendance_session_id: string | null, staffing_assignment_id: string | null, facility_id: string | null, work_date: string, component: string, rule_code: string | null, worked_seconds: number, base_hourly_rate: string, multiplier: string, hourly_adjustment: string, amount: string, description: string, };

export type PayrollRun = { id: string, period_start: string, period_end: string, time_zone: string, currency: string, status: PayrollRunStatus, calculated_at: string | null, approved_at: string | null, created_at: string, results: Array<PayrollEmployeeResult>, lines: Array<PayrollLine>, };

export type EmployeeCompensationCreateRequest = { currency: string, pay_basis: PayBasis, hourly_rate?: string | null, monthly_rate?: string | null, standard_monthly_hours?: string | null, effective_from: string, effective_to?: string | null, };

export type FacilityRateRuleCreateRequest = { code: string, name: string, facility_id: string, employee_id?: string | null, base_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to?: string | null, is_active: boolean, };

export type TimeBandRuleCreateRequest = { code: string, name: string, weekdays: Array<number>, start_time: string, end_time: string, spans_next_day: boolean, premium_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to?: string | null, is_active: boolean, };

export type OvertimeRuleCreateRequest = { code: string, name: string, threshold_minutes: number, premium_multiplier: string, hourly_adjustment: string, priority: number, effective_from: string, effective_to?: string | null, is_active: boolean, };

export type PayrollCalculateRequest = { year: number, month: number, time_zone: string, currency: string, };

export type EmployeeStatus = "active" | "on_leave" | "terminated";

export type HrRecordStatus = "active" | "archived";

export type Employee = { id: string, account_id: string | null, employee_code: string, display_name: string, work_email: string | null, work_phone: string | null, badge_id: string | null, status: EmployeeStatus, hire_date: string, termination_date: string | null, created_at: string, updated_at: string, };

export type Department = { id: string, code: string, name: string, parent_department_id: string | null, manager_employee_id: string | null, status: HrRecordStatus, created_at: string, updated_at: string, };

export type JobPosition = { id: string, code: string, name: string, department_id: string | null, status: HrRecordStatus, created_at: string, updated_at: string, };

export type EmployeeAssignment = { id: string, employee_id: string, branch_id: string, facility_id: string | null, department_id: string | null, job_id: string | null, manager_employee_id: string | null, date_start: string, date_end: string | null, is_primary: boolean, created_at: string, };

export type AttendanceSession = { id: string, employee_id: string, facility_id: string, check_in_at: string, check_out_at: string | null, worked_seconds: number | null, created_at: string, updated_at: string, };

export type AttendanceCheckInRequest = { facility_id: string, };

export type EmployeeUpsertRequest = { account_id?: string | null, employee_code: string, display_name: string, work_email?: string | null, work_phone?: string | null, badge_id?: string | null, status: EmployeeStatus, hire_date: string, termination_date?: string | null, };

export type DepartmentUpsertRequest = { code: string, name: string, parent_department_id?: string | null, manager_employee_id?: string | null, status: HrRecordStatus, };

export type JobPositionUpsertRequest = { code: string, name: string, department_id?: string | null, status: HrRecordStatus, };

export type EmployeeAssignmentCreateRequest = { branch_id: string, facility_id?: string | null, department_id?: string | null, job_id?: string | null, manager_employee_id?: string | null, date_start: string, date_end?: string | null, is_primary: boolean, };

export type WorkingPeriod = { id: string, weekday: number, start_time: string, end_time: string, spans_next_day: boolean, unpaid_break_minutes: number, };

export type WorkingSchedule = { id: string, code: string, name: string, time_zone: string, status: HrRecordStatus, periods: Array<WorkingPeriod>, created_at: string, updated_at: string, };

export type EmployeeScheduleAssignment = { id: string, employee_id: string, schedule_id: string, date_start: string, date_end: string | null, created_at: string, };

export type WorkingPeriodRequest = { weekday: number, start_time: string, end_time: string, spans_next_day: boolean, unpaid_break_minutes: number, };

export type EmployeeScheduleAssignmentView = { assignment: EmployeeScheduleAssignment, schedule: WorkingSchedule, };

export type WorkingScheduleUpsertRequest = { code: string, name: string, time_zone: string, status: HrRecordStatus, periods: Array<WorkingPeriodRequest>, };

export type EmployeeScheduleAssignmentCreateRequest = { schedule_id: string, date_start: string, date_end?: string | null, };

