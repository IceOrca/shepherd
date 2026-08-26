import type {
  Employee,
  EmployeeCitizenIdUpdateRequest,
  EmployeeSensitiveProfile,
  EmployeeUpsertRequest,
} from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export const peopleQueryKeys = {
  employees: ["people", "employees"] as const,
};

export function listEmployees(): Promise<Employee[]> {
  return apiRequest<Employee[]>("/api/hr/employees");
}

export function updateEmployee(employeeId: string, payload: EmployeeUpsertRequest): Promise<Employee> {
  return apiRequest<Employee>(`/api/hr/employees/${employeeId}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  });
}

export function getEmployeeCitizenId(employeeId: string): Promise<EmployeeSensitiveProfile> {
  return apiRequest<EmployeeSensitiveProfile>(`/api/hr/employees/${employeeId}/citizen-id`);
}

export function updateEmployeeCitizenId(
  employeeId: string,
  payload: EmployeeCitizenIdUpdateRequest,
): Promise<EmployeeSensitiveProfile> {
  return apiRequest<EmployeeSensitiveProfile>(`/api/hr/employees/${employeeId}/citizen-id`, {
    method: "PUT",
    body: JSON.stringify(payload),
  });
}
