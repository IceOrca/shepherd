import type {
  BranchSummary,
  Customer,
  CustomerUpsertRequest,
  CustomerWorkRecord,
  CustomerWorkRecordUpsertRequest,
  Employee,
  JobPosition,
  OwnStaffingAssignment,
  ShiftAssignment,
  ShiftAssignmentApproveRequest,
  ShiftAssignmentCreateRequest,
  ShiftWorkActionRequest,
  ShiftWorkSession,
  StaffingCandidate,
  StaffingEligibility,
  StaffingEligibilityCreateRequest,
  StaffingRate,
  StaffingRateCreateRequest,
  StaffingReconciliation,
  StaffingShift,
  StaffingShiftCreateRequest,
  UrgentCustomerWorkRecord,
  UrgentCustomerWorkRecordUpsertRequest,
  UrgentWorkEmployee,
  UrgentWorkEndRequest,
  UrgentWorkCustomer,
  UrgentWorkItem,
  UrgentWorkReconcileRequest,
  UrgentWorkReconciliation,
  UrgentWorkStartRequest,
} from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export const operationsQueryKeys = {
  all: ["operations"] as const,
  branches: ["operations", "branches"] as const,
  customers: ["operations", "customers"] as const,
  jobs: ["operations", "jobs"] as const,
  employees: ["operations", "employees"] as const,
  staffingRates: ["operations", "staffing-rates"] as const,
  staffingEligibilities: ["operations", "staffing-eligibilities"] as const,
  ownAssignments: ["operations", "own-assignments"] as const,
  reconciliations: ["operations", "reconciliations"] as const,
  urgentEmployees: ["operations", "urgent-work", "employees"] as const,
  urgentCustomers: ["operations", "urgent-work", "customers"] as const,
  urgentOwnWork: ["operations", "urgent-work", "me"] as const,
  urgentTeamWork: ["operations", "urgent-work", "team"] as const,
  urgentReconciliations: ["operations", "urgent-work", "reconciliations"] as const,
  shifts: ["operations", "shifts"] as const,
  candidates: (shiftId: string) => ["operations", "shifts", shiftId, "candidates"] as const,
};

export function listBranches(): Promise<BranchSummary[]> {
  return apiRequest<BranchSummary[]>("/api/business/branches");
}

export interface WorkActionInput {
  action: "start" | "end";
  assignmentId: string;
  idempotencyKey: string;
  payload: ShiftWorkActionRequest;
}

export interface UrgentStartActionInput {
  idempotencyKey: string;
  payload: UrgentWorkStartRequest;
}

export interface UrgentEndActionInput {
  idempotencyKey: string;
  reportId: string;
  payload: UrgentWorkEndRequest;
}

export function listCustomers(): Promise<Customer[]> {
  return apiRequest<Customer[]>("/api/business/customers");
}

export function createCustomer(payload: CustomerUpsertRequest): Promise<Customer> {
  return apiRequest<Customer>("/api/business/customers", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function updateCustomer(customerId: string, payload: CustomerUpsertRequest): Promise<Customer> {
  return apiRequest<Customer>(`/api/business/customers/${customerId}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  });
}

export function listJobs(): Promise<JobPosition[]> {
  return apiRequest<JobPosition[]>("/api/hr/jobs");
}

export function listEmployees(): Promise<Employee[]> {
  return apiRequest<Employee[]>("/api/hr/employees");
}

export function listStaffingRates(): Promise<StaffingRate[]> {
  return apiRequest<StaffingRate[]>("/api/business/staffing/rates");
}

export function createStaffingRate(payload: StaffingRateCreateRequest): Promise<StaffingRate> {
  return apiRequest<StaffingRate>("/api/business/staffing/rates", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function listStaffingEligibilities(): Promise<StaffingEligibility[]> {
  return apiRequest<StaffingEligibility[]>("/api/business/staffing/eligibilities");
}

export function createStaffingEligibility(
  payload: StaffingEligibilityCreateRequest,
): Promise<StaffingEligibility> {
  return apiRequest<StaffingEligibility>("/api/business/staffing/eligibilities", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function listStaffingShifts(): Promise<StaffingShift[]> {
  return apiRequest<StaffingShift[]>("/api/business/staffing/shifts");
}

export function createStaffingShift(payload: StaffingShiftCreateRequest): Promise<StaffingShift> {
  return apiRequest<StaffingShift>("/api/business/staffing/shifts", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function listShiftCandidates(shiftId: string): Promise<StaffingCandidate[]> {
  return apiRequest<StaffingCandidate[]>(`/api/business/staffing/shifts/${shiftId}/candidates`);
}

export function createShiftAssignment(
  shiftId: string,
  payload: ShiftAssignmentCreateRequest,
): Promise<ShiftAssignment> {
  return apiRequest<ShiftAssignment>(`/api/business/staffing/shifts/${shiftId}/assignments`, {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function listReconciliations(): Promise<StaffingReconciliation[]> {
  return apiRequest<StaffingReconciliation[]>("/api/business/staffing/reconciliations");
}

export function saveCustomerWorkRecord(
  assignmentId: string,
  payload: CustomerWorkRecordUpsertRequest,
): Promise<CustomerWorkRecord> {
  return apiRequest<CustomerWorkRecord>(
    `/api/business/staffing/assignments/${assignmentId}/customer-record`,
    { method: "PUT", body: JSON.stringify(payload) },
  );
}

export function reconcileAssignment(
  assignmentId: string,
  payload: ShiftAssignmentApproveRequest,
): Promise<ShiftAssignment> {
  return apiRequest<ShiftAssignment>(`/api/business/staffing/assignments/${assignmentId}/reconcile`, {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function listOwnAssignments(): Promise<OwnStaffingAssignment[]> {
  return apiRequest<OwnStaffingAssignment[]>("/api/business/staffing/assignments/me");
}

export function executeWorkAction(input: WorkActionInput): Promise<ShiftWorkSession> {
  return apiRequest<ShiftWorkSession>(
    `/api/business/staffing/assignments/${input.assignmentId}/${input.action}`,
    {
      method: "POST",
      headers: { "Idempotency-Key": input.idempotencyKey },
      body: JSON.stringify(input.payload),
    },
  );
}

export function listUrgentCustomers(): Promise<UrgentWorkCustomer[]> {
  return apiRequest<UrgentWorkCustomer[]>("/api/business/staffing/urgent-work/customers");
}

export function listUrgentEmployees(): Promise<UrgentWorkEmployee[]> {
  return apiRequest<UrgentWorkEmployee[]>("/api/business/staffing/urgent-work/employees");
}

export function listOwnUrgentWork(): Promise<UrgentWorkItem[]> {
  return apiRequest<UrgentWorkItem[]>("/api/business/staffing/urgent-work/me");
}

export function listTeamUrgentWork(): Promise<UrgentWorkItem[]> {
  return apiRequest<UrgentWorkItem[]>("/api/business/staffing/urgent-work/team");
}

export function startUrgentWork(input: UrgentStartActionInput): Promise<UrgentWorkItem[]> {
  return apiRequest<UrgentWorkItem[]>("/api/business/staffing/urgent-work/start", {
    method: "POST",
    headers: { "Idempotency-Key": input.idempotencyKey },
    body: JSON.stringify(input.payload),
  });
}

export function endUrgentWork(input: UrgentEndActionInput): Promise<UrgentWorkItem> {
  return apiRequest<UrgentWorkItem>(`/api/business/staffing/urgent-work/${input.reportId}/end`, {
    method: "POST",
    headers: { "Idempotency-Key": input.idempotencyKey },
    body: JSON.stringify(input.payload),
  });
}

export function listUrgentReconciliations(): Promise<UrgentWorkReconciliation[]> {
  return apiRequest<UrgentWorkReconciliation[]>("/api/business/staffing/urgent-work/reconciliations");
}

export function saveUrgentCustomerWorkRecord(
  reportId: string,
  payload: UrgentCustomerWorkRecordUpsertRequest,
): Promise<UrgentCustomerWorkRecord> {
  return apiRequest<UrgentCustomerWorkRecord>(
    `/api/business/staffing/urgent-work/${reportId}/customer-record`,
    { method: "PUT", body: JSON.stringify(payload) },
  );
}

export function reconcileUrgentWork(
  reportId: string,
  payload: UrgentWorkReconcileRequest,
): Promise<UrgentWorkReconciliation> {
  return apiRequest<UrgentWorkReconciliation>(`/api/business/staffing/urgent-work/${reportId}/reconcile`, {
    method: "POST",
    body: JSON.stringify(payload),
  });
}
