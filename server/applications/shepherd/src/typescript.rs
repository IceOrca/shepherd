use ts_rs::{Config, TS};

use crate::{
    auth::{
        AccessControlAuditEntry, AccessControlBranch, AccessControlPermission, AccessControlRole,
        AccessControlSnapshot, AccessControlUser, AccessRoleScope, AccountPermissionOverrideContract,
        AccountRoleAssignmentContract, AccountStatus, AuthProviderUserStatus, AuthUserSummary,
        CreateAccessControlBranchRequest, CreateAccessControlRoleRequest, CreateAuthUserRequest, CurrentUserProfile,
        PermissionCode, PermissionOverrideEffect, RoleCode, SetAuthUserStatusRequest, UpdateAccessControlBranchRequest,
        UpdateAccessControlRoleRequest, UpdateAccountAccessRequest, TenantMembershipSummary,
    },
    business::staffing::{
        core::{
            BusinessRecordStatus, Customer, CustomerWorkRecord, RateSource, ReconciliationStatus, ShiftAssignment,
            ShiftAssignmentStatus, StaffingCandidate, StaffingEligibility, StaffingPriceSet, StaffingRate, StaffingJob,
            StaffingRateKind, StaffingReconciliation, StaffingShift, StaffingShiftStatus, StaffingStaff,
        },
        host::{
            CustomerPageResponse, CustomerUpsertRequest, CustomerWorkRecordUpsertRequest, ManualRateOverrideRequest,
            ShiftAssignmentApproveRequest, ShiftAssignmentCreateRequest, StaffingEligibilityCreateRequest,
            StaffingPriceSetRequest, StaffingRatePageResponse, StaffingReconciliationPageResponse,
            StaffingShiftCreateRequest, StaffingStaffPageResponse,
        },
        urgent_work::{
            core::{
                UrgentCustomerWorkRecord, UrgentWorkActionSource, UrgentWorkCustomer, UrgentWorkEmployee,
                UrgentWorkItem, UrgentWorkReconciliation, UrgentWorkStatus,
            },
            host::{
                UrgentCustomerWorkRecordUpsertRequest, UrgentWorkAcceptStaffRecordRequest, UrgentWorkEndRequest,
                UrgentReconciliationPageResponse, UrgentWorkReconcileRequest, UrgentWorkStartRequest,
            },
        },
        work_session::{
            core::{OwnStaffingAssignment, ShiftWorkSession},
            host::ShiftWorkActionRequest,
        },
    },
    business::finance::{
        core::{
            ExpenseCategory, ExpenseClaim, ExpenseClaimRevision, ExpenseClaimStatus, ExpenseFundingSource,
            SalaryAdvance, SalaryAdvanceRecoverySource, SalaryAdvanceRevision, SalaryAdvanceStatus,
        },
        host::{
            ExpenseClaimCreateRequest, ExpenseCorrectionRequest, ExpensePageResponse, ExpenseRevisionPageResponse,
            FinancialDecisionRequest, FinancialRejectionRequest, FinancialSettlementRequest,
            SalaryAdvanceCorrectionRequest, SalaryAdvanceCreateRequest, SalaryAdvanceDisbursementRequest,
            SalaryAdvancePageResponse, SalaryAdvanceRecoveryRequest, SalaryAdvanceRevisionPageResponse,
        },
        reporting::{
            core::{
                EmployeeSalaryConfiguration, FinancialPeriodState, FinancialPeriodStatus, OperatingFinancialLine,
                OperatingFinancialReport, PayrollLine, PayrollReport,
            },
            export::ReportExportKind,
            host::{EmployeeSalaryRateCreateRequest, FinancialPeriodChangeRequest, FinancialReportExportRequest},
        },
    },
    features::{
        organization::core::BranchSummary,
        people::{
            core::{AttendanceSession, Employee, EmployeeSensitiveProfile, EmployeeStatus, Gender},
            host::handler::{AttendancePageResponse, EmployeePageResponse},
            host::dto::{AttendanceCheckInRequest, EmployeeCitizenIdUpdateRequest, EmployeeUpsertRequest},
        },
    },
};

pub fn contract() -> String {
    let config = Config::new().with_large_int("number");
    let mut output = String::new();

    push::<RoleCode>(&mut output, &config);
    push::<PermissionCode>(&mut output, &config);
    push::<AccountStatus>(&mut output, &config);
    push::<AuthProviderUserStatus>(&mut output, &config);
    push::<CurrentUserProfile>(&mut output, &config);
    push::<TenantMembershipSummary>(&mut output, &config);
    push::<AuthUserSummary>(&mut output, &config);
    push::<CreateAuthUserRequest>(&mut output, &config);
    push::<SetAuthUserStatusRequest>(&mut output, &config);
    push::<AccessRoleScope>(&mut output, &config);
    push::<PermissionOverrideEffect>(&mut output, &config);
    push::<AccessControlBranch>(&mut output, &config);
    push::<AccessControlPermission>(&mut output, &config);
    push::<AccessControlRole>(&mut output, &config);
    push::<AccountRoleAssignmentContract>(&mut output, &config);
    push::<AccountPermissionOverrideContract>(&mut output, &config);
    push::<AccessControlUser>(&mut output, &config);
    push::<AccessControlAuditEntry>(&mut output, &config);
    push::<AccessControlSnapshot>(&mut output, &config);
    push::<CreateAccessControlBranchRequest>(&mut output, &config);
    push::<UpdateAccessControlBranchRequest>(&mut output, &config);
    push::<CreateAccessControlRoleRequest>(&mut output, &config);
    push::<UpdateAccessControlRoleRequest>(&mut output, &config);
    push::<UpdateAccountAccessRequest>(&mut output, &config);

    push::<BranchSummary>(&mut output, &config);
    push::<BusinessRecordStatus>(&mut output, &config);
    push::<StaffingShiftStatus>(&mut output, &config);
    push::<ShiftAssignmentStatus>(&mut output, &config);
    push::<RateSource>(&mut output, &config);
    push::<StaffingRateKind>(&mut output, &config);
    push::<Customer>(&mut output, &config);
    push::<CustomerPageResponse>(&mut output, &config);
    push::<StaffingJob>(&mut output, &config);
    push::<StaffingRate>(&mut output, &config);
    push::<StaffingRatePageResponse>(&mut output, &config);
    push::<StaffingStaff>(&mut output, &config);
    push::<StaffingStaffPageResponse>(&mut output, &config);
    push::<StaffingPriceSet>(&mut output, &config);
    push::<StaffingShift>(&mut output, &config);
    push::<ShiftAssignment>(&mut output, &config);
    push::<StaffingCandidate>(&mut output, &config);
    push::<StaffingEligibility>(&mut output, &config);
    push::<ReconciliationStatus>(&mut output, &config);
    push::<CustomerWorkRecord>(&mut output, &config);
    push::<StaffingReconciliation>(&mut output, &config);
    push::<StaffingReconciliationPageResponse>(&mut output, &config);
    push::<CustomerWorkRecordUpsertRequest>(&mut output, &config);
    push::<CustomerUpsertRequest>(&mut output, &config);
    push::<StaffingPriceSetRequest>(&mut output, &config);
    push::<StaffingEligibilityCreateRequest>(&mut output, &config);
    push::<StaffingShiftCreateRequest>(&mut output, &config);
    push::<ManualRateOverrideRequest>(&mut output, &config);
    push::<ShiftAssignmentCreateRequest>(&mut output, &config);
    push::<ShiftAssignmentApproveRequest>(&mut output, &config);
    push::<ShiftWorkSession>(&mut output, &config);
    push::<OwnStaffingAssignment>(&mut output, &config);
    push::<ShiftWorkActionRequest>(&mut output, &config);
    push::<ExpenseFundingSource>(&mut output, &config);
    push::<ExpenseClaimStatus>(&mut output, &config);
    push::<SalaryAdvanceRecoverySource>(&mut output, &config);
    push::<SalaryAdvanceStatus>(&mut output, &config);
    push::<ExpenseCategory>(&mut output, &config);
    push::<ExpenseClaim>(&mut output, &config);
    push::<ExpensePageResponse>(&mut output, &config);
    push::<ExpenseClaimRevision>(&mut output, &config);
    push::<ExpenseRevisionPageResponse>(&mut output, &config);
    push::<SalaryAdvance>(&mut output, &config);
    push::<SalaryAdvancePageResponse>(&mut output, &config);
    push::<SalaryAdvanceRevision>(&mut output, &config);
    push::<SalaryAdvanceRevisionPageResponse>(&mut output, &config);
    push::<ExpenseClaimCreateRequest>(&mut output, &config);
    push::<ExpenseCorrectionRequest>(&mut output, &config);
    push::<FinancialDecisionRequest>(&mut output, &config);
    push::<FinancialRejectionRequest>(&mut output, &config);
    push::<FinancialSettlementRequest>(&mut output, &config);
    push::<SalaryAdvanceCreateRequest>(&mut output, &config);
    push::<SalaryAdvanceCorrectionRequest>(&mut output, &config);
    push::<SalaryAdvanceDisbursementRequest>(&mut output, &config);
    push::<SalaryAdvanceRecoveryRequest>(&mut output, &config);
    push::<EmployeeSalaryConfiguration>(&mut output, &config);
    push::<EmployeeSalaryRateCreateRequest>(&mut output, &config);
    push::<FinancialPeriodStatus>(&mut output, &config);
    push::<FinancialPeriodState>(&mut output, &config);
    push::<FinancialPeriodChangeRequest>(&mut output, &config);
    push::<OperatingFinancialLine>(&mut output, &config);
    push::<OperatingFinancialReport>(&mut output, &config);
    push::<PayrollLine>(&mut output, &config);
    push::<PayrollReport>(&mut output, &config);
    push::<ReportExportKind>(&mut output, &config);
    push::<FinancialReportExportRequest>(&mut output, &config);
    push::<UrgentWorkStatus>(&mut output, &config);
    push::<UrgentWorkActionSource>(&mut output, &config);
    push::<UrgentWorkCustomer>(&mut output, &config);
    push::<UrgentWorkEmployee>(&mut output, &config);
    push::<UrgentWorkItem>(&mut output, &config);
    push::<UrgentCustomerWorkRecord>(&mut output, &config);
    push::<UrgentWorkReconciliation>(&mut output, &config);
    push::<UrgentReconciliationPageResponse>(&mut output, &config);
    push::<UrgentWorkStartRequest>(&mut output, &config);
    push::<UrgentWorkEndRequest>(&mut output, &config);
    push::<UrgentCustomerWorkRecordUpsertRequest>(&mut output, &config);
    push::<UrgentWorkReconcileRequest>(&mut output, &config);
    push::<UrgentWorkAcceptStaffRecordRequest>(&mut output, &config);
    push::<EmployeeStatus>(&mut output, &config);
    push::<Gender>(&mut output, &config);
    push::<Employee>(&mut output, &config);
    push::<EmployeeSensitiveProfile>(&mut output, &config);
    push::<AttendanceSession>(&mut output, &config);
    push::<AttendancePageResponse>(&mut output, &config);
    push::<EmployeePageResponse>(&mut output, &config);
    push::<AttendanceCheckInRequest>(&mut output, &config);
    push::<EmployeeUpsertRequest>(&mut output, &config);
    push::<EmployeeCitizenIdUpdateRequest>(&mut output, &config);

    output
}

fn push<T: TS>(output: &mut String, config: &Config) {
    output.push_str("export ");
    output.push_str(&T::decl(config));
    output.push_str("\n\n");
}
