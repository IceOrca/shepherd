// This file is generated from Rust API DTOs. Do not edit it manually.

export type RoleCode = string;

export type PermissionCode = string;

export type AccountStatus = "active" | "disabled";

export type AuthProviderUserStatus = "active" | "disabled" | "missing";

export type CurrentUserProfile = { tenant_id: string, account_id: string, username: string, email: string | null, primary_role: RoleCode, roles: Array<RoleCode>, permissions: Array<PermissionCode>, branch_ids: Array<string>, active_branch_id: string | null, };

export type TenantMembershipSummary = { tenant_id: string, account_id: string, tenant_slug: string, tenant_display_name: string, username: string, email: string | null, primary_role: RoleCode, };

export type AuthUserSummary = { auth_user_id: string, account_id: string, username: string, email: string | null, primary_role: RoleCode, branch_ids: Array<string>, account_status: AccountStatus, provider_status: AuthProviderUserStatus, email_confirmed: boolean, created_at: string | null, last_sign_in_at: string | null, };

export type AuthUserPage = { items: Array<AuthUserSummary>, next_cursor: string | null, has_more: boolean, limit: number, };

export type CreateAuthUserRequest = { username: string, email: string, password: string | null, primary_role: RoleCode, branch_ids: Array<string>, additional_role_assignments: Array<AccountRoleAssignmentContract>, };

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

export type AccessControlSnapshot = { branches: Array<AccessControlBranch>, permissions: Array<AccessControlPermission>, roles: Array<AccessControlRole>, users: Array<AccessControlUser>, audit: Array<AccessControlAuditEntry>, role_next_cursor: string | null, role_has_more: boolean, user_next_cursor: string | null, user_has_more: boolean, audit_next_cursor: string | null, audit_has_more: boolean, limit: number, };

export type CreateAccessControlRoleRequest = { code: RoleCode, display_name: string, description: string | null, scope: AccessRoleScope, permission_codes: Array<PermissionCode>, };

export type UpdateAccessControlRoleRequest = { display_name: string, description: string | null, is_active: boolean, expected_version: number, permission_codes: Array<PermissionCode>, };

export type UpdateAccountAccessRequest = { primary_role: RoleCode, expected_version: number, assignments: Array<AccountRoleAssignmentContract>, permission_overrides: Array<AccountPermissionOverrideContract>, };

export type BranchSummary = { id: string, code: string, name: string, time_zone: string, };

export type Branch = { id: string, code: string, name: string, time_zone: string, status: string, version: number, };

export type BranchCreateRequest = { code: string, name: string, time_zone: string, };

export type BranchUpdateRequest = { name: string, time_zone: string, status: string, expected_version: number, };

export type BranchPageResponse = { items: Array<Branch>, next_cursor: string | null, has_more: boolean, limit: number, };

export type BranchSummaryPageResponse = { items: Array<BranchSummary>, next_cursor: string | null, has_more: boolean, limit: number, };

export type BusinessRecordStatus = "active" | "disabled";

export type StaffingShiftStatus = "open" | "filled" | "in_progress" | "completed" | "cancelled";

export type ShiftAssignmentStatus = "assigned" | "approved" | "cancelled";

export type RateSource = "configured" | "manual";

export type StaffingRateKind = "customer_bill" | "worker_pay";

export type Customer = { id: string, code: string, name: string, address: string | null, time_zone: string, billing_email: string | null, status: BusinessRecordStatus, version: number, created_at: string, updated_at: string, };

export type CustomerPageResponse = { items: Array<Customer>, next_cursor: string | null, has_more: boolean, limit: number, };

export type StaffingJob = { id: string, code: string, name: string, status: BusinessRecordStatus, created_at: string, updated_at: string, };

export type StaffingRate = { id: string, rate_kind: StaffingRateKind, code: string, name: string, customer_id: string | null, employee_id: string | null, currency: string, hourly_rate: string, priority: number, effective_from: string, effective_to: string | null, is_active: boolean, created_at: string, };

export type StaffingRatePageResponse = { items: Array<StaffingRate>, next_cursor: string | null, has_more: boolean, limit: number, };

export type StaffingStaff = { employee_id: string, employee_code: string, display_name: string, };

export type StaffingStaffPageResponse = { items: Array<StaffingStaff>, next_cursor: string | null, has_more: boolean, limit: number, };

export type StaffingPriceSet = { customer_bill_rate: StaffingRate, worker_pay_rate: StaffingRate, };

export type StaffingShift = { id: string, customer_id: string, job_id: string, starts_at: string, ends_at: string, required_workers: number, status: StaffingShiftStatus, notes: string | null, created_at: string, updated_at: string, };

export type ShiftAssignment = { id: string, shift_id: string, employee_id: string, customer_bill_rate_id: string | null, worker_pay_rate_id: string | null, rate_source: RateSource, manual_rate_reason: string | null, currency: string, bill_hourly_rate_snapshot: string, worker_hourly_rate_snapshot: string, eligibility_exception_reason: string | null, status: ShiftAssignmentStatus, worked_seconds: number | null, observed_worked_seconds: number | null, approval_adjustment_reason: string | null, customer_amount: string | null, worker_amount: string | null, margin_amount: string | null, approved_at: string | null, created_at: string, };

export type StaffingCandidate = { employee_id: string, employee_code: string, display_name: string, suitable: boolean, available: boolean, already_assigned: boolean, conflict_shift_id: string | null, };

export type ReconcileStatus = "pending_staff" | "pending_customer" | "matched" | "discrepancy" | "reconciled";

export type CustomerWorkRecord = { id: string, assignment_id: string, confirmed_customer_id: string, confirmed_started_at: string, confirmed_ended_at: string, confirmed_worked_seconds: number, customer_reference: string | null, notes: string | null, updated_at: string, };

export type StaffingReconcile = { assignment_id: string, shift_id: string, customer_id: string, job_id: string, employee_id: string, employee_code: string, employee_name: string, customer_name: string, confirmed_customer_name: string | null, scheduled_starts_at: string, scheduled_ends_at: string, assignment_status: ShiftAssignmentStatus, staff_started_at: string | null, staff_ended_at: string | null, staff_worked_seconds: number, customer_record: CustomerWorkRecord | null, final_worked_seconds: number | null, final_customer_id: string | null, final_job_id: string | null, adjustment_reason: string | null, reconciliation_status: ReconcileStatus, result_revision_id: string | null, result_revision_number: number | null, };

export type StaffingReconcilePageRsp = { items: Array<StaffingReconcile>, next_cursor: string | null, has_more: boolean, limit: number, };

export type CustomerWorkRecordUpsertReq = { confirmed_customer_id: string, confirmed_started_at: string, confirmed_ended_at: string, customer_reference?: string | null, notes?: string | null, };

export type ReconciliationCorrectionReq = { expected_revision_id: string, worked_seconds: number, correction_reason: string, };

export type ReconciliationRevision = { revision_id: string, assignment_id: string, revision_number: number, worked_seconds: number, correction_reason: string | null, recorded_at: string, };

export type CustomerUpsertRequest = { code: string, name: string, address?: string | null, time_zone: string, billing_email?: string | null, status: BusinessRecordStatus, expected_version?: number | null, };

export type StaffingPriceSetRequest = { customer_id: string, employee_id?: string | null, currency: string, customer_hourly_rate: string, worker_hourly_rate: string, effective_from: string, };

export type StaffingCancellationRequest = { reason: string, };

export type StaffingShiftCreateRequest = { customer_id: string, job_id: string, starts_at: string, ends_at: string, required_workers: number, notes?: string | null, };

export type ManualRateOverrideRequest = { reason: string, currency: string, bill_hourly_rate: string, worker_hourly_rate: string, };

export type ShiftAssignmentCreateRequest = { employee_id: string, manual_rate?: ManualRateOverrideRequest | null, };

export type ShiftAssignmentApproveRequest = { worked_seconds?: number | null, adjustment_reason?: string | null, final_customer_id?: string | null, final_job_id?: string | null, };

export type ShiftWorkSession = { id: string, assignment_id: string, employee_id: string, started_at: string, ended_at: string | null, worked_seconds: number | null, started_latitude: number | null, started_longitude: number | null, started_accuracy_meters: number | null, ended_latitude: number | null, ended_longitude: number | null, ended_accuracy_meters: number | null, created_at: string, updated_at: string, };

export type OwnStaffingAssignment = { assignment_id: string, shift_id: string, customer_name: string, starts_at: string, ends_at: string, status: ShiftAssignmentStatus, observed_worked_seconds: number, is_working: boolean, staff_started_at: string | null, staff_ended_at: string | null, };

export type OwnStaffingAssignmentPageResponse = { items: Array<OwnStaffingAssignment>, next_cursor: string | null, has_more: boolean, limit: number, };

export type ShiftWorkActionRequest = { latitude?: number | null, longitude?: number | null, accuracy_meters?: number | null, };

export type ExpenseFundingSource = "company_funds" | "employee_personal";

export type ExpenseClaimStatus = "submitted" | "approved" | "rejected" | "cancelled";

export type SalaryAdvanceRecoverySource = "manual_repayment";

export type SalaryAdvanceStatus = "requested" | "approved" | "disbursed" | "recovered" | "rejected" | "cancelled";

export type ExpenseCategory = { id: string, code: string, display_name: string, };

export type ExpenseClaim = { id: string, branch_id: string, category_id: string, category_name: string, funding_source: ExpenseFundingSource, paid_by_employee_id: string | null, paid_by_employee_name: string | null, customer_id: string | null, urgent_work_report_id: string | null, staffing_assignment_id: string | null, paid_on: string, payroll_inclusion_on: string, description: string, evidence_reference: string | null, claimed_amount: string, approved_amount: string | null, reimbursed_amount: string, outstanding_reimbursement: string, currency: string, status: ExpenseClaimStatus, decision_reason: string | null, submitted_by_account_id: string, submitted_by_username: string, approved_by_username: string | null, approved_at: string | null, revision_id: string, revision_number: number, revision_kind: string, correction_reason: string | null, revised_by_username: string, revised_at: string, financial_period_open: boolean, created_at: string, updated_at: string, };

export type ExpensePageRsp = { items: Array<ExpenseClaim>, next_cursor: string | null, has_more: boolean, limit: number, };

export type ExpenseClaimRevision = { revision_id: string, revision_number: number, revision_kind: string, correction_reason: string | null, revised_by_username: string, revised_at: string, category_name: string, paid_on: string, payroll_inclusion_on: string, description: string, claimed_amount: string, approved_amount: string | null, currency: string, status: ExpenseClaimStatus, };

export type ExpenseRevisionPageRsp = { items: Array<ExpenseClaimRevision>, next_cursor: string | null, has_more: boolean, limit: number, };

export type SalaryAdvance = { id: string, branch_id: string, employee_id: string, employee_code: string, employee_name: string, requested_amount: string, approved_amount: string | null, recovered_amount: string, outstanding_amount: string, currency: string, reason: string, paid_on: string, payroll_inclusion_on: string, status: SalaryAdvanceStatus, decision_reason: string | null, requested_by_username: string, approved_by_username: string | null, disbursed_by_username: string | null, disbursement_reference: string | null, requested_at: string, approved_at: string | null, disbursed_at: string | null, revision_id: string, revision_number: number, revision_kind: string, correction_reason: string | null, revised_by_username: string, revised_at: string, financial_period_open: boolean, updated_at: string, };

export type SalaryAdvancePageResponse = { items: Array<SalaryAdvance>, next_cursor: string | null, has_more: boolean, limit: number, };

export type SalaryAdvanceRevision = { revision_id: string, revision_number: number, revision_kind: string, correction_reason: string | null, revised_by_username: string, revised_at: string, employee_name: string, requested_amount: string, approved_amount: string | null, currency: string, reason: string, paid_on: string, payroll_inclusion_on: string, status: SalaryAdvanceStatus, };

export type SalaryAdvanceRevisionPageResponse = { items: Array<SalaryAdvanceRevision>, next_cursor: string | null, has_more: boolean, limit: number, };

export type ExpenseClaimCreateReq = { category_id: string, funding_source: ExpenseFundingSource, paid_by_employee_id?: string | null, customer_id?: string | null, urgent_work_report_id?: string | null, staffing_assignment_id?: string | null, paid_on: string, payroll_inclusion_on: string, description: string, evidence_reference?: string | null, claimed_amount: string, currency: string, };

export type ExpenseCorrectionReq = { expected_revision_id: string, correction_reason: string, category_id: string, funding_source: ExpenseFundingSource, paid_by_employee_id?: string | null, customer_id?: string | null, urgent_work_report_id?: string | null, staffing_assignment_id?: string | null, paid_on: string, payroll_inclusion_on: string, description: string, evidence_reference?: string | null, claimed_amount: string, approved_amount?: string | null, currency: string, };

export type FinancialDecisionReq = { approved_amount: string, reason?: string | null, };

export type FinancialRejectionRequest = { reason: string, };

export type FinancialSettlementReq = { amount: string, reference: string, };

export type SalaryAdvanceCreateReq = { employee_id: string, requested_amount: string, currency: string, reason: string, paid_on: string, payroll_inclusion_on: string, };

export type SalaryAdvanceCorrectionReq = { expected_revision_id: string, correction_reason: string, employee_id: string, requested_amount: string, approved_amount?: string | null, currency: string, reason: string, paid_on: string, payroll_inclusion_on: string, };

export type SalaryAdvanceDisburseReq = { reference: string, };

export type SalaryAdvanceRecoveryReq = { amount: string, source: SalaryAdvanceRecoverySource, reference: string, };

export type EmployeeSalaryConfig = { employee_id: string, branch_id: string, employee_code: string, employee_name: string, role: RoleCode, rate_id: string | null, monthly_amount: string | null, currency: string | null, effective_from: string | null, effective_to: string | null, };

export type EmployeeSalaryConfigPageRsp = { items: Array<EmployeeSalaryConfig>, next_cursor: string | null, has_more: boolean, limit: number, };

export type EmployeeSalaryRateCreateReq = { employee_id: string, monthly_amount: string, currency: string, effective_from: string, };

export type FinancialPeriodStatus = "open" | "closed";

export type FinancialPeriodState = { branch_id: string, period_start: string, status: FinancialPeriodStatus, revision_number: number, reason: string | null, actor_username: string | null, occurred_at: string | null, };

export type FinancialPeriodChangeRequest = { period_start: string, status: FinancialPeriodStatus, expected_revision_number: number, reason: string, };

export type OperatingFinancialLine = { currency: string, staffing_revenue: string, staffing_worker_cost: string, coordination_salary_cost: string, approved_business_expense: string, profit_share_cost: string, operating_cost: string, operating_profit: string, business_profit_after_profit_share: string, reimbursed_cash: string, salary_advance_disbursed: string, salary_advance_recovered: string, outstanding_expense_reimbursement: string, outstanding_salary_advance: string, };

export type OperatingFinancialReport = { branch_id: string, branch_name: string, start_date: string, end_date: string, lines: Array<OperatingFinancialLine>, };

export type PayrollLine = { employee_id: string, branch_id: string, employee_code: string, employee_name: string, role: RoleCode, currency: string, staffing_worked_seconds: number, staffing_earnings: string, prorated_monthly_salary: string, profit_share_base: string, profit_share_percent: string, profit_share_payment: string, profit_share_locked: boolean, gross_pay: string, recorded_expense_reimbursement: string, suggested_expense_reimbursement: string, recorded_advance_deduction: string, outstanding_advance_due: string, suggested_advance_deduction: string, estimated_net_pay: string, attendance_overlap_count: number, };

export type PayrollReport = { branch_id: string, branch_name: string, start_date: string, end_date: string, lines: Array<PayrollLine>, };

export type ReportExportKind = "operating_financial" | "payroll";

export type FinancialReportExportReq = { report_kind: ReportExportKind, start_date: string, end_date: string, branch_ids: Array<string>, };

export type UrgentWorkStatus = "active" | "completed" | "reconciled" | "cancelled";

export type UrgentWorkActionSource = "self_reported" | "peer";

export type UrgentWorkSubmissionKind = "live" | "manual";

export type UrgentWorkCustomer = { customer_id: string, customer_name: string, address: string | null, time_zone: string, };

export type UrgentWorkEmployee = { employee_id: string, employee_code: string, display_name: string, is_self: boolean, has_open_work: boolean, };

export type UrgentWorkItem = { report_id: string, branch_id: string, branch_name: string, employee_id: string, employee_code: string, employee_name: string, claimed_customer_id: string, customer_name: string, submission_kind: UrgentWorkSubmissionKind, staff_note: string | null, status: UrgentWorkStatus, started_at: string, ended_at: string | null, worked_seconds: number | null, started_by_account_id: string, started_by_username: string, start_source: UrgentWorkActionSource, ended_by_account_id: string | null, ended_by_username: string | null, end_source: UrgentWorkActionSource | null, reconciled_assignment_id: string | null, created_at: string, updated_at: string, };

export type UrgentOwnWorkPageRsp = { items: Array<UrgentWorkItem>, next_cursor: string | null, has_more: boolean, limit: number, };

export type UrgentCustomerWorkRecord = { id: string, report_id: string, confirmed_customer_id: string, confirmed_customer_name: string, confirmed_started_at: string, confirmed_ended_at: string, confirmed_worked_seconds: number, customer_reference: string | null, notes: string | null, updated_at: string, };

export type UrgentWorkReconcile = { work: UrgentWorkItem, customer_record: UrgentCustomerWorkRecord | null, reconciliation_status: ReconcileStatus, final_customer_id: string | null, final_job_id: string | null, final_worked_seconds: number | null, adjustment_reason: string | null, eligibility_exception_reason: string | null, result_revision_id: string | null, result_revision_number: number | null, };

export type UrgentReconcileRsp = { items: Array<UrgentWorkReconcile>, next_cursor: string | null, has_more: boolean, limit: number, };

export type UrgentWorkStartReq = { customer_id: string, employee_ids: Array<string>, latitude: number | null, longitude: number | null, accuracy_meters: number | null, };

export type UrgentWorkEndReq = { latitude: number | null, longitude: number | null, accuracy_meters: number | null, };

export type UrgentWorkManualReq = { customer_id: string, started_at: string, ended_at: string, note?: string | null, };

export type UrgentCustomerWorkRecordUpsertReq = { confirmed_customer_id: string, confirmed_started_at: string, confirmed_ended_at: string, customer_reference?: string | null, notes?: string | null, };

export type UrgentWorkReconcileReq = { final_customer_id: string, job_id: string, worked_seconds: number, adjustment_reason?: string | null, manual_rate?: ManualRateOverrideRequest | null, };

export type UrgentWorkAcceptStaffRecordReq = { job_id: string, };

export type UrgentWorkCancellationReq = { reason: string, };

export type EmployeeStatus = "active" | "on_leave" | "terminated";

export type Gender = "female" | "male" | "other" | "unspecified";

export type Employee = { id: string, branch_id: string, account_id: string | null, employee_code: string, display_name: string, legal_first_name: string | null, legal_middle_name: string | null, legal_last_name: string | null, personal_phone_e164: string | null, gender: Gender | null, citizen_id_country_code: string | null, citizen_id_last4: string | null, profile_complete: boolean, status: EmployeeStatus, hire_date: string, termination_date: string | null, version: number, created_at: string, updated_at: string, };

export type EmployeeSensitiveProfile = { employee_id: string, citizen_id_country_code: string | null, citizen_id: string | null, version: number, };

export type AttendanceSession = { id: string, employee_id: string, branch_id: string, check_in_at: string, check_out_at: string | null, worked_seconds: number | null, created_at: string, updated_at: string, };

export type AttendancePageResponse = { items: Array<AttendanceSession>, next_cursor: string | null, has_more: boolean, limit: number, };

export type EmployeePageResponse = { items: Array<Employee>, next_cursor: string | null, has_more: boolean, limit: number, };

export type AttendanceCheckInRequest = { branch_id: string, };

export type EmployeeUpsertRequest = { account_id?: string | null, employee_code: string, display_name: string, legal_first_name?: string | null, legal_middle_name?: string | null, legal_last_name?: string | null, personal_phone_e164?: string | null, gender?: Gender | null, status: EmployeeStatus, hire_date: string, termination_date?: string | null, expected_version?: number | null, };

export type EmployeeCitizenIdUpdateRequest = { citizen_id_country_code?: string | null, citizen_id?: string | null, expected_version: number, };

