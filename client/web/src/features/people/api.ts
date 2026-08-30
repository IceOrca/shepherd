import type {
  Employee,
  EmployeePageResponse,
  EmployeeCitizenIdUpdateRequest,
  EmployeeSensitiveProfile,
  EmployeeUpsertRequest,
} from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export const peopleQueryKeys = {
  employees: ["people", "employees"] as const,
};

export function listEmployees(cursor: string | null, search: string): Promise<EmployeePageResponse> {
  const parameters: URLSearchParams = new URLSearchParams();
  if (cursor !== null) parameters.set("cursor", cursor);
  if (search.trim() !== "") parameters.set("search", search.trim());
  const query: string = parameters.toString();
  return apiRequest<EmployeePageResponse>(`/api/hr/employees${query ? `?${query}` : ""}`);
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
