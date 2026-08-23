use ts_rs::{Config, TS};

use crate::{
    auth::{
        AccountStatus, AuthProviderUserStatus, AuthUserSummary, CreateAuthUserRequest, CurrentUserProfile,
        PermissionCode, RoleCode, SetAuthUserStatusRequest,
    },
    business::staffing::{
        core::{
            BusinessRecordStatus, Customer, CustomerWorkRecord, RateSource, ReconciliationStatus, ShiftAssignment,
            ShiftAssignmentStatus, StaffingCandidate, StaffingEligibility, StaffingRate, StaffingRateKind,
            StaffingReconciliation, StaffingShift, StaffingShiftStatus,
        },
        host::{
            CustomerUpsertRequest, CustomerWorkRecordUpsertRequest, ManualRateOverrideRequest,
            ShiftAssignmentApproveRequest, ShiftAssignmentCreateRequest, StaffingEligibilityCreateRequest,
            StaffingRateCreateRequest, StaffingShiftCreateRequest,
        },
        urgent_work::{
            core::{
                UrgentCustomerWorkRecord, UrgentWorkActionSource, UrgentWorkCustomer, UrgentWorkEmployee,
                UrgentWorkItem, UrgentWorkReconciliation, UrgentWorkStatus,
            },
            host::{
                UrgentCustomerWorkRecordUpsertRequest, UrgentWorkEndRequest, UrgentWorkReconcileRequest,
                UrgentWorkStartRequest,
            },
        },
        work_session::{
            core::{OwnStaffingAssignment, ShiftWorkSession},
            host::ShiftWorkActionRequest,
        },
    },
    features::{
        organization::core::BranchSummary,
        payroll::{
            core::{
                BranchRateRule, EmployeeCompensation, OvertimeRule, PayBasis, PayrollEmployeeResult, PayrollLine,
                PayrollRun, PayrollRunStatus, TimeBandRule,
            },
            host::{
                BranchRateRuleCreateRequest, EmployeeCompensationCreateRequest, OvertimeRuleCreateRequest,
                PayrollCalculateRequest, TimeBandRuleCreateRequest,
            },
        },
        people::{
            core::{
                AttendanceSession, Department, Employee, EmployeeAssignment, EmployeeStatus, HrRecordStatus,
                JobPosition,
            },
            host::dto::{
                AttendanceCheckInRequest, DepartmentUpsertRequest, EmployeeAssignmentCreateRequest,
                EmployeeUpsertRequest, JobPositionUpsertRequest,
            },
        },
        working_schedule::{
            core::{EmployeeScheduleAssignment, WorkingPeriod, WorkingSchedule},
            host::dto::{
                EmployeeScheduleAssignmentCreateRequest, EmployeeScheduleAssignmentView, WorkingPeriodRequest,
                WorkingScheduleUpsertRequest,
            },
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
    push::<AuthUserSummary>(&mut output, &config);
    push::<CreateAuthUserRequest>(&mut output, &config);
    push::<SetAuthUserStatusRequest>(&mut output, &config);

    push::<BranchSummary>(&mut output, &config);
    push::<BusinessRecordStatus>(&mut output, &config);
    push::<StaffingShiftStatus>(&mut output, &config);
    push::<ShiftAssignmentStatus>(&mut output, &config);
    push::<RateSource>(&mut output, &config);
    push::<StaffingRateKind>(&mut output, &config);
    push::<Customer>(&mut output, &config);
    push::<StaffingRate>(&mut output, &config);
    push::<StaffingShift>(&mut output, &config);
    push::<ShiftAssignment>(&mut output, &config);
    push::<StaffingCandidate>(&mut output, &config);
    push::<StaffingEligibility>(&mut output, &config);
    push::<ReconciliationStatus>(&mut output, &config);
    push::<CustomerWorkRecord>(&mut output, &config);
    push::<StaffingReconciliation>(&mut output, &config);
    push::<CustomerWorkRecordUpsertRequest>(&mut output, &config);
    push::<CustomerUpsertRequest>(&mut output, &config);
    push::<StaffingRateCreateRequest>(&mut output, &config);
    push::<StaffingEligibilityCreateRequest>(&mut output, &config);
    push::<StaffingShiftCreateRequest>(&mut output, &config);
    push::<ManualRateOverrideRequest>(&mut output, &config);
    push::<ShiftAssignmentCreateRequest>(&mut output, &config);
    push::<ShiftAssignmentApproveRequest>(&mut output, &config);
    push::<ShiftWorkSession>(&mut output, &config);
    push::<OwnStaffingAssignment>(&mut output, &config);
    push::<ShiftWorkActionRequest>(&mut output, &config);
    push::<UrgentWorkStatus>(&mut output, &config);
    push::<UrgentWorkActionSource>(&mut output, &config);
    push::<UrgentWorkCustomer>(&mut output, &config);
    push::<UrgentWorkEmployee>(&mut output, &config);
    push::<UrgentWorkItem>(&mut output, &config);
    push::<UrgentCustomerWorkRecord>(&mut output, &config);
    push::<UrgentWorkReconciliation>(&mut output, &config);
    push::<UrgentWorkStartRequest>(&mut output, &config);
    push::<UrgentWorkEndRequest>(&mut output, &config);
    push::<UrgentCustomerWorkRecordUpsertRequest>(&mut output, &config);
    push::<UrgentWorkReconcileRequest>(&mut output, &config);
    push::<PayBasis>(&mut output, &config);
    push::<PayrollRunStatus>(&mut output, &config);
    push::<EmployeeCompensation>(&mut output, &config);
    push::<BranchRateRule>(&mut output, &config);
    push::<TimeBandRule>(&mut output, &config);
    push::<OvertimeRule>(&mut output, &config);
    push::<PayrollEmployeeResult>(&mut output, &config);
    push::<PayrollLine>(&mut output, &config);
    push::<PayrollRun>(&mut output, &config);
    push::<EmployeeCompensationCreateRequest>(&mut output, &config);
    push::<BranchRateRuleCreateRequest>(&mut output, &config);
    push::<TimeBandRuleCreateRequest>(&mut output, &config);
    push::<OvertimeRuleCreateRequest>(&mut output, &config);
    push::<PayrollCalculateRequest>(&mut output, &config);
    push::<EmployeeStatus>(&mut output, &config);
    push::<HrRecordStatus>(&mut output, &config);
    push::<Employee>(&mut output, &config);
    push::<Department>(&mut output, &config);
    push::<JobPosition>(&mut output, &config);
    push::<EmployeeAssignment>(&mut output, &config);
    push::<AttendanceSession>(&mut output, &config);
    push::<AttendanceCheckInRequest>(&mut output, &config);
    push::<EmployeeUpsertRequest>(&mut output, &config);
    push::<DepartmentUpsertRequest>(&mut output, &config);
    push::<JobPositionUpsertRequest>(&mut output, &config);
    push::<EmployeeAssignmentCreateRequest>(&mut output, &config);
    push::<WorkingPeriod>(&mut output, &config);
    push::<WorkingSchedule>(&mut output, &config);
    push::<EmployeeScheduleAssignment>(&mut output, &config);
    push::<WorkingPeriodRequest>(&mut output, &config);
    push::<EmployeeScheduleAssignmentView>(&mut output, &config);
    push::<WorkingScheduleUpsertRequest>(&mut output, &config);
    push::<EmployeeScheduleAssignmentCreateRequest>(&mut output, &config);

    output
}

fn push<T: TS>(output: &mut String, config: &Config) {
    output.push_str("export ");
    output.push_str(&T::decl(config));
    output.push_str("\n\n");
}
