import type {
  Customer,
  CustomerFacility,
  CustomerWorkRecord,
  CustomerWorkRecordUpsertRequest,
  JobPosition,
  OwnStaffingAssignment,
  ShiftAssignment,
  ShiftAssignmentApproveRequest,
  ShiftAssignmentCreateRequest,
  ShiftWorkActionRequest,
  ShiftWorkSession,
  StaffingCandidate,
  StaffingReconciliation,
  StaffingShift,
  StaffingShiftCreateRequest,
  UrgentCustomerWorkRecord,
  UrgentCustomerWorkRecordUpsertRequest,
  UrgentWorkEmployee,
  UrgentWorkEndRequest,
  UrgentWorkFacility,
  UrgentWorkItem,
  UrgentWorkReconcileRequest,
  UrgentWorkReconciliation,
  UrgentWorkStartRequest,
} from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export const operationsQueryKeys = {
  all: ["operations"] as const,
  customers: ["operations", "customers"] as const,
  jobs: ["operations", "jobs"] as const,
  ownAssignments: ["operations", "own-assignments"] as const,
  reconciliations: ["operations", "reconciliations"] as const,
  urgentEmployees: ["operations", "urgent-work", "employees"] as const,
  urgentFacilities: ["operations", "urgent-work", "facilities"] as const,
  urgentOwnWork: ["operations", "urgent-work", "me"] as const,
  urgentTeamWork: ["operations", "urgent-work", "team"] as const,
  urgentReconciliations: ["operations", "urgent-work", "reconciliations"] as const,
  shifts: ["operations", "shifts"] as const,
  facilities: (customerId: string) => ["operations", "customers", customerId, "facilities"] as const,
  candidates: (shiftId: string) => ["operations", "shifts", shiftId, "candidates"] as const,
};

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

export function listCustomerFacilities(customerId: string): Promise<CustomerFacility[]> {
  return apiRequest<CustomerFacility[]>(`/api/business/customers/${customerId}/facilities`);
}

export function listJobs(): Promise<JobPosition[]> {
  return apiRequest<JobPosition[]>("/api/hr/jobs");
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

export function listUrgentFacilities(): Promise<UrgentWorkFacility[]> {
  return apiRequest<UrgentWorkFacility[]>("/api/business/staffing/urgent-work/facilities");
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
