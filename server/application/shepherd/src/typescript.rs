use ts_rs::{Config, TS};

use crate::{
    auth::{
        AccessControlAuditEntry, AccessControlBranch, AccessControlPermission, AccessControlRole,
        AccessControlSnapshot, AccessControlUser, AccessRoleScope, AccountPermissionOverrideContract,
        AccountRoleAssignmentContract, AccountStatus, AuthProviderUserStatus, AuthUserPage, AuthUserSummary,
        CreateAccessControlBranchRequest, CreateAccessControlRoleRequest, CreateAuthUserRequest, CurrentUserProfile,
        PermissionCode, PermissionOverrideEffect, RoleCode, SetAuthUserStatusRequest, UpdateAccessControlBranchRequest,
        UpdateAccessControlRoleRequest, UpdateAccountAccessRequest, TenantMembershipSummary,
    },
    business::staffing::{
        core::{
            BusinessRecordStatus, Customer, CustomerWorkRecord, RateSource, ReconcileStatus, ReconciliationRevision,
            ShiftAssignment, ShiftAssignmentStatus, StaffingCandidate, StaffingJob, StaffingPriceSet, StaffingRate,
            StaffingRateKind, StaffingReconcile, StaffingShift, StaffingShiftStatus, StaffingStaff,
        },
        host::{
            CustomerPageResponse, CustomerUpsertRequest, CustomerWorkRecordUpsertReq, ManualRateOverrideRequest,
            ReconciliationCorrectionReq, ShiftAssignmentApproveRequest, ShiftAssignmentCreateRequest,
            StaffingCancellationRequest, StaffingPriceSetRequest, StaffingRatePageResponse, StaffingReconcilePageRsp,
            StaffingShiftCreateRequest, StaffingStaffPageResponse,
        },
        urgent_work::{
            core::{
                UrgentCustomerWorkRecord, UrgentWorkActionSource, UrgentWorkCustomer, UrgentWorkEmployee,
                UrgentWorkItem, UrgentWorkReconcile, UrgentWorkStatus, UrgentWorkSubmissionKind,
            },
            host::{
                UrgentCustomerWorkRecordUpsertReq, UrgentWorkAcceptStaffRecordReq, UrgentWorkEndReq,
                UrgentOwnWorkPageRsp, UrgentReconcileRsp, UrgentWorkCancellationReq, UrgentWorkManualReq,
                UrgentWorkReconcileReq, UrgentWorkStartReq,
            },
        },
        work_session::{
            core::{OwnStaffingAssignment, ShiftWorkSession},
            host::{OwnStaffingAssignmentPageResponse, ShiftWorkActionRequest},
        },
    },
    business::finance::{
        core::{
            ExpenseCategory, ExpenseClaim, ExpenseClaimRevision, ExpenseClaimStatus, ExpenseFundingSource,
            SalaryAdvance, SalaryAdvanceRecoverySource, SalaryAdvanceRevision, SalaryAdvanceStatus,
        },
        host::{
            ExpenseClaimCreateReq, ExpenseCorrectionReq, ExpensePageRsp, ExpenseRevisionPageRsp, FinancialDecisionReq,
            FinancialRejectionRequest, FinancialSettlementReq, SalaryAdvanceCorrectionReq, SalaryAdvanceCreateReq,
            SalaryAdvanceDisburseReq, SalaryAdvancePageResponse, SalaryAdvanceRecoveryReq,
            SalaryAdvanceRevisionPageResponse,
        },
        reporting::{
            core::{
                EmployeeSalaryConfig, FinancialPeriodState, FinancialPeriodStatus, OperatingFinancialLine,
                OperatingFinancialReport, PayrollLine, PayrollReport,
            },
            export::ReportExportKind,
            host::{
                EmployeeSalaryConfigPageRsp, EmployeeSalaryRateCreateReq, FinancialPeriodChangeRequest,
                FinancialReportExportReq,
            },
        },
    },
    features::{
        branch::core::BranchSummary,
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
    push::<AuthUserPage>(&mut output, &config);
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
    push::<ReconcileStatus>(&mut output, &config);
    push::<CustomerWorkRecord>(&mut output, &config);
    push::<StaffingReconcile>(&mut output, &config);
    push::<StaffingReconcilePageRsp>(&mut output, &config);
    push::<CustomerWorkRecordUpsertReq>(&mut output, &config);
    push::<ReconciliationCorrectionReq>(&mut output, &config);
    push::<ReconciliationRevision>(&mut output, &config);
    push::<CustomerUpsertRequest>(&mut output, &config);
    push::<StaffingPriceSetRequest>(&mut output, &config);
    push::<StaffingCancellationRequest>(&mut output, &config);
    push::<StaffingShiftCreateRequest>(&mut output, &config);
    push::<ManualRateOverrideRequest>(&mut output, &config);
    push::<ShiftAssignmentCreateRequest>(&mut output, &config);
    push::<ShiftAssignmentApproveRequest>(&mut output, &config);
    push::<ShiftWorkSession>(&mut output, &config);
    push::<OwnStaffingAssignment>(&mut output, &config);
    push::<OwnStaffingAssignmentPageResponse>(&mut output, &config);
    push::<ShiftWorkActionRequest>(&mut output, &config);
    push::<ExpenseFundingSource>(&mut output, &config);
    push::<ExpenseClaimStatus>(&mut output, &config);
    push::<SalaryAdvanceRecoverySource>(&mut output, &config);
    push::<SalaryAdvanceStatus>(&mut output, &config);
    push::<ExpenseCategory>(&mut output, &config);
    push::<ExpenseClaim>(&mut output, &config);
    push::<ExpensePageRsp>(&mut output, &config);
    push::<ExpenseClaimRevision>(&mut output, &config);
    push::<ExpenseRevisionPageRsp>(&mut output, &config);
    push::<SalaryAdvance>(&mut output, &config);
    push::<SalaryAdvancePageResponse>(&mut output, &config);
    push::<SalaryAdvanceRevision>(&mut output, &config);
    push::<SalaryAdvanceRevisionPageResponse>(&mut output, &config);
    push::<ExpenseClaimCreateReq>(&mut output, &config);
    push::<ExpenseCorrectionReq>(&mut output, &config);
    push::<FinancialDecisionReq>(&mut output, &config);
    push::<FinancialRejectionRequest>(&mut output, &config);
    push::<FinancialSettlementReq>(&mut output, &config);
    push::<SalaryAdvanceCreateReq>(&mut output, &config);
    push::<SalaryAdvanceCorrectionReq>(&mut output, &config);
    push::<SalaryAdvanceDisburseReq>(&mut output, &config);
    push::<SalaryAdvanceRecoveryReq>(&mut output, &config);
    push::<EmployeeSalaryConfig>(&mut output, &config);
    push::<EmployeeSalaryConfigPageRsp>(&mut output, &config);
    push::<EmployeeSalaryRateCreateReq>(&mut output, &config);
    push::<FinancialPeriodStatus>(&mut output, &config);
    push::<FinancialPeriodState>(&mut output, &config);
    push::<FinancialPeriodChangeRequest>(&mut output, &config);
    push::<OperatingFinancialLine>(&mut output, &config);
    push::<OperatingFinancialReport>(&mut output, &config);
    push::<PayrollLine>(&mut output, &config);
    push::<PayrollReport>(&mut output, &config);
    push::<ReportExportKind>(&mut output, &config);
    push::<FinancialReportExportReq>(&mut output, &config);
    push::<UrgentWorkStatus>(&mut output, &config);
    push::<UrgentWorkActionSource>(&mut output, &config);
    push::<UrgentWorkSubmissionKind>(&mut output, &config);
    push::<UrgentWorkCustomer>(&mut output, &config);
    push::<UrgentWorkEmployee>(&mut output, &config);
    push::<UrgentWorkItem>(&mut output, &config);
    push::<UrgentOwnWorkPageRsp>(&mut output, &config);
    push::<UrgentCustomerWorkRecord>(&mut output, &config);
    push::<UrgentWorkReconcile>(&mut output, &config);
    push::<UrgentReconcileRsp>(&mut output, &config);
    push::<UrgentWorkStartReq>(&mut output, &config);
    push::<UrgentWorkEndReq>(&mut output, &config);
    push::<UrgentWorkManualReq>(&mut output, &config);
    push::<UrgentCustomerWorkRecordUpsertReq>(&mut output, &config);
    push::<UrgentWorkReconcileReq>(&mut output, &config);
    push::<UrgentWorkAcceptStaffRecordReq>(&mut output, &config);
    push::<UrgentWorkCancellationReq>(&mut output, &config);
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
