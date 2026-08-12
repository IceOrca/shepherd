import type {
  Customer,
  OwnStaffingAssignment,
  ShiftWorkActionRequest,
  ShiftWorkSession,
  StaffingShift,
} from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export const operationsQueryKeys = {
  all: ["operations"] as const,
  customers: ["operations", "customers"] as const,
  ownAssignments: ["operations", "own-assignments"] as const,
  shifts: ["operations", "shifts"] as const,
};

export interface WorkActionInput {
  action: "start" | "end";
  assignmentId: string;
  idempotencyKey: string;
  payload: ShiftWorkActionRequest;
}

export function listCustomers(): Promise<Customer[]> {
  return apiRequest<Customer[]>("/business/customers");
}

export function listStaffingShifts(): Promise<StaffingShift[]> {
  return apiRequest<StaffingShift[]>("/business/staffing/shifts");
}

export function listOwnAssignments(): Promise<OwnStaffingAssignment[]> {
  return apiRequest<OwnStaffingAssignment[]>("/business/staffing/assignments/me");
}

export function executeWorkAction(input: WorkActionInput): Promise<ShiftWorkSession> {
  return apiRequest<ShiftWorkSession>(
    `/business/staffing/assignments/${input.assignmentId}/${input.action}`,
    {
      method: "POST",
      headers: { "Idempotency-Key": input.idempotencyKey },
      body: JSON.stringify(input.payload),
    },
  );
}
