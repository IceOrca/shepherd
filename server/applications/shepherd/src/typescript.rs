use ts_rs::{Config, TS};

use crate::{
    auth::{AuthUserSummary, CreateAuthUserRequest, CurrentUserProfile, SetAuthUserStatusRequest},
    business::staffing::{
        core::{
            BusinessRecordStatus, Customer, CustomerFacility, CustomerWorkRecord, RateSource, ReconciliationStatus,
            ShiftAssignment, ShiftAssignmentStatus, StaffingCandidate, StaffingRateAgreement, StaffingReconciliation,
            StaffingShift, StaffingShiftStatus,
        },
        host::{
            CustomerCreateRequest, CustomerFacilityCreateRequest, CustomerWorkRecordUpsertRequest,
            ManualRateOverrideRequest, ShiftAssignmentApproveRequest, ShiftAssignmentCreateRequest,
            StaffingRateAgreementCreateRequest, StaffingShiftCreateRequest,
        },
        work_session::{
            core::{OwnStaffingAssignment, ShiftWorkSession},
            host::ShiftWorkActionRequest,
        },
    },
    features::{
        organization::core::{BranchSummary, FacilitySummary},
        payroll::{
            core::{
                EmployeeCompensation, FacilityRateRule, OvertimeRule, PayBasis, PayrollEmployeeResult, PayrollLine,
                PayrollRun, PayrollRunStatus, TimeBandRule,
            },
            host::{
                EmployeeCompensationCreateRequest, FacilityRateRuleCreateRequest, OvertimeRuleCreateRequest,
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

    push::<CurrentUserProfile>(&mut output, &config);
    push::<AuthUserSummary>(&mut output, &config);
    push::<CreateAuthUserRequest>(&mut output, &config);
    push::<SetAuthUserStatusRequest>(&mut output, &config);

    push::<BranchSummary>(&mut output, &config);
    push::<FacilitySummary>(&mut output, &config);
    push::<BusinessRecordStatus>(&mut output, &config);
    push::<StaffingShiftStatus>(&mut output, &config);
    push::<ShiftAssignmentStatus>(&mut output, &config);
    push::<RateSource>(&mut output, &config);
    push::<Customer>(&mut output, &config);
    push::<CustomerFacility>(&mut output, &config);
    push::<StaffingRateAgreement>(&mut output, &config);
    push::<StaffingShift>(&mut output, &config);
    push::<ShiftAssignment>(&mut output, &config);
    push::<StaffingCandidate>(&mut output, &config);
    push::<ReconciliationStatus>(&mut output, &config);
    push::<CustomerWorkRecord>(&mut output, &config);
    push::<StaffingReconciliation>(&mut output, &config);
    push::<CustomerWorkRecordUpsertRequest>(&mut output, &config);
    push::<CustomerCreateRequest>(&mut output, &config);
    push::<CustomerFacilityCreateRequest>(&mut output, &config);
    push::<StaffingRateAgreementCreateRequest>(&mut output, &config);
    push::<StaffingShiftCreateRequest>(&mut output, &config);
    push::<ManualRateOverrideRequest>(&mut output, &config);
    push::<ShiftAssignmentCreateRequest>(&mut output, &config);
    push::<ShiftAssignmentApproveRequest>(&mut output, &config);
    push::<ShiftWorkSession>(&mut output, &config);
    push::<OwnStaffingAssignment>(&mut output, &config);
    push::<ShiftWorkActionRequest>(&mut output, &config);
    push::<PayBasis>(&mut output, &config);
    push::<PayrollRunStatus>(&mut output, &config);
    push::<EmployeeCompensation>(&mut output, &config);
    push::<FacilityRateRule>(&mut output, &config);
    push::<TimeBandRule>(&mut output, &config);
    push::<OvertimeRule>(&mut output, &config);
    push::<PayrollEmployeeResult>(&mut output, &config);
    push::<PayrollLine>(&mut output, &config);
    push::<PayrollRun>(&mut output, &config);
    push::<EmployeeCompensationCreateRequest>(&mut output, &config);
    push::<FacilityRateRuleCreateRequest>(&mut output, &config);
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
