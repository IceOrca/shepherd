import type {
  BranchSummary,
  Customer,
  CustomerPageResponse,
  CustomerUpsertRequest,
  CustomerWorkRecord,
  CustomerWorkRecordUpsertReq,
  ReconciliationCorrectionReq,
  ReconciliationRevision,
  Employee,
  EmployeePageResponse,
  StaffingJob,
  OwnStaffingAssignment,
  ShiftAssignment,
  ShiftAssignmentApproveRequest,
  ShiftAssignmentCreateRequest,
  ShiftWorkActionRequest,
  ShiftWorkSession,
  StaffingCandidate,
  StaffingEligibility,
  StaffingEligibilityCreateRequest,
  StaffingPriceSet,
  StaffingPriceSetRequest,
  StaffingRate,
  StaffingRatePageResponse,
  StaffingReconcilePageRsp,
  StaffingShift,
  StaffingShiftCreateRequest,
  StaffingStaff,
  StaffingStaffPageResponse,
  UrgentCustomerWorkRecord,
  UrgentCustomerWorkRecordUpsertReq as UrgentCustomerWorkRecordUpsertRequest,
  UrgentOwnWorkPageRsp,
  UrgentWorkAcceptStaffRecordReq as UrgentWorkAcceptStaffRecordRequest,
  UrgentWorkEmployee,
  UrgentWorkEndReq as UrgentWorkEndRequest,
  UrgentWorkCustomer,
  UrgentWorkItem,
  UrgentWorkManualReq as UrgentWorkManualRequest,
  UrgentWorkReconcileReq as UrgentWorkReconcileRequest,
  UrgentWorkReconcile,
  UrgentReconcileRsp as UrgentReconcilePageRsp,
  UrgentWorkStartReq as UrgentWorkStartRequest,
} from "../../api/generated/contracts";
import { apiRequest, apiRequestForBranch } from "../../shared/api/client";
import { collectCursorPages } from "../../shared/api/pagination";

export const operationsQueryKeys = {
  all: ["operations"] as const,
  branches: ["operations", "branches"] as const,
  customers: ["operations", "customers"] as const,
  jobs: ["operations", "jobs"] as const,
  employees: ["operations", "employees"] as const,
  staffingRates: ["operations", "staffing-rates"] as const,
  staffingStaff: ["operations", "staffing-staff"] as const,
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

export interface UrgentManualActionInput {
  idempotencyKey: string;
  payload: UrgentWorkManualRequest;
}

function customerPagePath(cursor: string | null, search: string): string {
  const parameters: URLSearchParams = new URLSearchParams();
  if (cursor !== null) parameters.set("cursor", cursor);
  if (search.trim() !== "") parameters.set("search", search.trim());
  const query: string = parameters.toString();
  return `/api/business/customers${query ? `?${query}` : ""}`;
}

export function listCustomersPage(
  cursor: string | null,
  search: string,
): Promise<CustomerPageResponse> {
  return apiRequest<CustomerPageResponse>(customerPagePath(cursor, search));
}

export function listCustomers(): Promise<Customer[]> {
  return collectCursorPages<Customer>((cursor: string | null): Promise<CustomerPageResponse> =>
    listCustomersPage(cursor, ""),
  );
}

export function listCustomersForBranch(branchId: string): Promise<Customer[]> {
  return collectCursorPages<Customer>((cursor: string | null): Promise<CustomerPageResponse> =>
    apiRequestForBranch<CustomerPageResponse>(customerPagePath(cursor, ""), branchId),
  );
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

export function listJobs(): Promise<StaffingJob[]> {
  return apiRequest<StaffingJob[]>("/api/business/staffing/jobs");
}

export function listJobsForBranch(branchId: string): Promise<StaffingJob[]> {
  return apiRequestForBranch<StaffingJob[]>("/api/business/staffing/jobs", branchId);
}

export function listEmployees(): Promise<Employee[]> {
  return collectCursorPages<Employee>((cursor: string | null): Promise<EmployeePageResponse> => {
    const path: string = cursor === null
      ? "/api/hr/employees"
      : `/api/hr/employees?cursor=${encodeURIComponent(cursor)}`;
    return apiRequest<EmployeePageResponse>(path);
  });
}

export function listStaffingRates(customerId: string): Promise<StaffingRate[]> {
  return collectCursorPages<StaffingRate>((cursor: string | null): Promise<StaffingRatePageResponse> => {
    const parameters: URLSearchParams = new URLSearchParams({ customer_id: customerId });
    if (cursor !== null) parameters.set("cursor", cursor);
    return apiRequest<StaffingRatePageResponse>(`/api/business/staffing/rates?${parameters.toString()}`);
  });
}

export function listStaffingStaff(
  cursor: string | null,
  search: string,
): Promise<StaffingStaffPageResponse> {
  const parameters: URLSearchParams = new URLSearchParams();
  if (cursor !== null) parameters.set("cursor", cursor);
  if (search.trim() !== "") parameters.set("search", search.trim());
  const query: string = parameters.toString();
  return apiRequest<StaffingStaffPageResponse>(`/api/business/staffing/staff${query ? `?${query}` : ""}`);
}

export function setStaffingPrices(payload: StaffingPriceSetRequest): Promise<StaffingPriceSet> {
  return apiRequest<StaffingPriceSet>("/api/business/staffing/prices", {
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

export function listStaffingShiftsForBranch(branchId: string): Promise<StaffingShift[]> {
  return apiRequestForBranch<StaffingShift[]>("/api/business/staffing/shifts", branchId);
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

export function listShiftCandidatesForBranch(
  branchId: string,
  shiftId: string,
): Promise<StaffingCandidate[]> {
  return apiRequestForBranch<StaffingCandidate[]>(
    `/api/business/staffing/shifts/${shiftId}/candidates`,
    branchId,
  );
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

export function createShiftAssignmentForBranch(
  branchId: string,
  shiftId: string,
  payload: ShiftAssignmentCreateRequest,
): Promise<ShiftAssignment> {
  return apiRequestForBranch<ShiftAssignment>(
    `/api/business/staffing/shifts/${shiftId}/assignments`,
    branchId,
    { method: "POST", body: JSON.stringify(payload) },
  );
}

function reconciliationPagePath(
  path: string,
  cursor: string | null,
  customerId: string | null,
  collection: "pending" | "confirmed" = "pending",
  periodStart: string | null = null,
  periodEnd: string | null = null,
): string {
  const parameters: URLSearchParams = new URLSearchParams();
  if (cursor !== null) {
    parameters.set("cursor", cursor);
  }
  if (customerId !== null) {
    parameters.set("customer_id", customerId);
  }
  parameters.set("collection", collection);
  if (periodStart !== null) parameters.set("period_start", periodStart);
  if (periodEnd !== null) parameters.set("period_end", periodEnd);
  const query: string = parameters.toString();
  return query.length === 0 ? path : `${path}?${query}`;
}

export function listReconciliations(
  cursor: string | null = null,
  customerId: string | null = null,
): Promise<StaffingReconcilePageRsp> {
  return apiRequest<StaffingReconcilePageRsp>(
    reconciliationPagePath("/api/business/staffing/reconciliations", cursor, customerId),
  );
}

export function listReconciliationsForBranch(
  branchId: string,
  cursor: string | null = null,
  customerId: string | null = null,
  collection: "pending" | "confirmed" = "pending",
  periodStart: string | null = null,
  periodEnd: string | null = null,
): Promise<StaffingReconcilePageRsp> {
  return apiRequestForBranch<StaffingReconcilePageRsp>(
    reconciliationPagePath("/api/business/staffing/reconciliations", cursor, customerId, collection, periodStart, periodEnd),
    branchId,
  );
}

export function saveCustomerWorkRecord(
  assignmentId: string,
  payload: CustomerWorkRecordUpsertReq,
): Promise<CustomerWorkRecord> {
  return apiRequest<CustomerWorkRecord>(
    `/api/business/staffing/assignments/${assignmentId}/customer-record`,
    { method: "PUT", body: JSON.stringify(payload) },
  );
}

export function saveCustomerWorkRecordForBranch(
  branchId: string,
  assignmentId: string,
  payload: CustomerWorkRecordUpsertReq,
): Promise<CustomerWorkRecord> {
  return apiRequestForBranch<CustomerWorkRecord>(
    `/api/business/staffing/assignments/${assignmentId}/customer-record`,
    branchId,
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

export function reconcileAssignmentForBranch(
  branchId: string,
  assignmentId: string,
  payload: ShiftAssignmentApproveRequest,
): Promise<ShiftAssignment> {
  return apiRequestForBranch<ShiftAssignment>(
    `/api/business/staffing/assignments/${assignmentId}/reconcile`,
    branchId,
    { method: "POST", body: JSON.stringify(payload) },
  );
}

export function acceptAssignmentStaffRecordForBranch(
  branchId: string,
  assignmentId: string,
): Promise<ShiftAssignment> {
  return apiRequestForBranch<ShiftAssignment>(
    `/api/business/staffing/assignments/${assignmentId}/accept-staff-record`,
    branchId,
    { method: "POST" },
  );
}

export function correctReconciliationForBranch(
  branchId: string,
  assignmentId: string,
  payload: ReconciliationCorrectionReq,
): Promise<ReconciliationRevision> {
  return apiRequestForBranch<ReconciliationRevision>(
    `/api/business/staffing/assignments/${assignmentId}/reconciliation-corrections`,
    branchId,
    { method: "POST", body: JSON.stringify(payload) },
  );
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

export function listUrgentCustomersForBranch(branchId: string): Promise<UrgentWorkCustomer[]> {
  return apiRequestForBranch<UrgentWorkCustomer[]>("/api/business/staffing/urgent-work/customers", branchId);
}

export function listUrgentEmployees(): Promise<UrgentWorkEmployee[]> {
  return apiRequest<UrgentWorkEmployee[]>("/api/business/staffing/urgent-work/employees");
}

export function listOwnUrgentWork(cursor: string | null = null): Promise<UrgentOwnWorkPageRsp> {
  const query: string = cursor === null ? "" : `?cursor=${encodeURIComponent(cursor)}`;
  return apiRequest<UrgentOwnWorkPageRsp>(`/api/business/staffing/urgent-work/me${query}`);
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

export function submitManualUrgentWork(input: UrgentManualActionInput): Promise<UrgentWorkItem> {
  return apiRequest<UrgentWorkItem>("/api/business/staffing/urgent-work/manual", {
    method: "POST",
    headers: { "Idempotency-Key": input.idempotencyKey },
    body: JSON.stringify(input.payload),
  });
}

export function listUrgentReconciliations(
  cursor: string | null = null,
  customerId: string | null = null,
): Promise<UrgentReconcilePageRsp> {
  return apiRequest<UrgentReconcilePageRsp>(
    reconciliationPagePath("/api/business/staffing/urgent-work/reconciliations", cursor, customerId),
  );
}

export function listUrgentReconciliationsForBranch(
  branchId: string,
  cursor: string | null = null,
  customerId: string | null = null,
  collection: "pending" | "confirmed" = "pending",
  periodStart: string | null = null,
  periodEnd: string | null = null,
): Promise<UrgentReconcilePageRsp> {
  return apiRequestForBranch<UrgentReconcilePageRsp>(
    reconciliationPagePath("/api/business/staffing/urgent-work/reconciliations", cursor, customerId, collection, periodStart, periodEnd),
    branchId,
  );
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

export function saveUrgentCustomerWorkRecordForBranch(
  branchId: string,
  reportId: string,
  payload: UrgentCustomerWorkRecordUpsertRequest,
): Promise<UrgentCustomerWorkRecord> {
  return apiRequestForBranch<UrgentCustomerWorkRecord>(
    `/api/business/staffing/urgent-work/${reportId}/customer-record`,
    branchId,
    { method: "PUT", body: JSON.stringify(payload) },
  );
}

export function reconcileUrgentWork(
  reportId: string,
  payload: UrgentWorkReconcileRequest,
): Promise<UrgentWorkReconcile> {
  return apiRequest<UrgentWorkReconcile>(`/api/business/staffing/urgent-work/${reportId}/reconcile`, {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function reconcileUrgentWorkForBranch(
  branchId: string,
  reportId: string,
  payload: UrgentWorkReconcileRequest,
): Promise<UrgentWorkReconcile> {
  return apiRequestForBranch<UrgentWorkReconcile>(
    `/api/business/staffing/urgent-work/${reportId}/reconcile`,
    branchId,
    { method: "POST", body: JSON.stringify(payload) },
  );
}

export function acceptUrgentStaffRecordForBranch(
  branchId: string,
  reportId: string,
  payload: UrgentWorkAcceptStaffRecordRequest,
): Promise<UrgentWorkReconcile> {
  return apiRequestForBranch<UrgentWorkReconcile>(
    `/api/business/staffing/urgent-work/${reportId}/accept-staff-record`,
    branchId,
    { method: "POST", body: JSON.stringify(payload) },
  );
}
