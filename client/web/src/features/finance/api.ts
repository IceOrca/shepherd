import type {
  ExpenseCategory,
  ExpenseClaim,
  ExpenseClaimCreateRequest,
  Employee,
  EmployeeSalaryConfiguration,
  EmployeeSalaryRateCreateRequest,
  FinancialDecisionRequest,
  FinancialRejectionRequest,
  FinancialSettlementRequest,
  OperatingFinancialReport,
  PayrollReport,
  SalaryAdvance,
  SalaryAdvanceCreateRequest,
  SalaryAdvanceDisbursementRequest,
  SalaryAdvanceRecoveryRequest,
} from "../../api/generated/contracts";
import { apiRequest, apiRequestForBranch } from "../../shared/api/client";

export const financeQueryKeys = {
  all: ["finance"] as const,
  expenseCategories: ["finance", "expense-categories"] as const,
  expenses: ["finance", "expenses"] as const,
  salaryAdvances: ["finance", "salary-advances"] as const,
  salaryConfigurations: ["finance", "salary-configurations"] as const,
  operatingReport: ["finance", "operating-report"] as const,
  payrollReport: ["finance", "payroll-report"] as const,
};

function mutationHeaders(): HeadersInit {
  return { "Idempotency-Key": crypto.randomUUID() };
}

export function listExpenseCategories(): Promise<ExpenseCategory[]> {
  return apiRequest<ExpenseCategory[]>("/api/business/finance/expense-categories");
}

export function getOwnEmployee(): Promise<Employee> {
  return apiRequest<Employee>("/api/hr/employees/me");
}

export function listExpenses(): Promise<ExpenseClaim[]> {
  return apiRequest<ExpenseClaim[]>("/api/business/finance/expenses");
}

export function createExpense(payload: ExpenseClaimCreateRequest): Promise<ExpenseClaim> {
  return apiRequest<ExpenseClaim>("/api/business/finance/expenses", {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function approveExpense(expenseId: string, payload: FinancialDecisionRequest): Promise<ExpenseClaim> {
  return apiRequest<ExpenseClaim>(`/api/business/finance/expenses/${expenseId}/approve`, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function rejectExpense(expenseId: string, payload: FinancialRejectionRequest): Promise<ExpenseClaim> {
  return apiRequest<ExpenseClaim>(`/api/business/finance/expenses/${expenseId}/reject`, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function reimburseExpense(expenseId: string, payload: FinancialSettlementRequest): Promise<ExpenseClaim> {
  return apiRequest<ExpenseClaim>(`/api/business/finance/expenses/${expenseId}/reimburse`, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function listSalaryAdvances(): Promise<SalaryAdvance[]> {
  return apiRequest<SalaryAdvance[]>("/api/business/finance/salary-advances");
}

export function createSalaryAdvance(payload: SalaryAdvanceCreateRequest): Promise<SalaryAdvance> {
  return apiRequest<SalaryAdvance>("/api/business/finance/salary-advances", {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function approveSalaryAdvance(
  advanceId: string,
  payload: FinancialDecisionRequest,
): Promise<SalaryAdvance> {
  return apiRequest<SalaryAdvance>(`/api/business/finance/salary-advances/${advanceId}/approve`, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function rejectSalaryAdvance(
  advanceId: string,
  payload: FinancialRejectionRequest,
): Promise<SalaryAdvance> {
  return apiRequest<SalaryAdvance>(`/api/business/finance/salary-advances/${advanceId}/reject`, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function disburseSalaryAdvance(
  advanceId: string,
  payload: SalaryAdvanceDisbursementRequest,
): Promise<SalaryAdvance> {
  return apiRequest<SalaryAdvance>(`/api/business/finance/salary-advances/${advanceId}/disburse`, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function recoverSalaryAdvance(
  advanceId: string,
  payload: SalaryAdvanceRecoveryRequest,
): Promise<SalaryAdvance> {
  return apiRequest<SalaryAdvance>(`/api/business/finance/salary-advances/${advanceId}/recover`, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function listSalaryConfigurations(): Promise<EmployeeSalaryConfiguration[]> {
  return apiRequest<EmployeeSalaryConfiguration[]>("/api/business/finance/salary-configurations");
}

export function createEmployeeSalaryRate(
  payload: EmployeeSalaryRateCreateRequest,
): Promise<EmployeeSalaryConfiguration> {
  return apiRequest<EmployeeSalaryConfiguration>("/api/business/finance/salary-configurations", {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

function reportQuery(startDate: string, endDate: string): string {
  const params = new URLSearchParams({ start_date: startDate, end_date: endDate });
  return params.toString();
}

export function getOperatingReportForBranch(
  branchId: string,
  startDate: string,
  endDate: string,
): Promise<OperatingFinancialReport> {
  return apiRequestForBranch<OperatingFinancialReport>(
    `/api/business/finance/operating-report?${reportQuery(startDate, endDate)}`,
    branchId,
  );
}

export function getPayrollReportForBranch(
  branchId: string,
  startDate: string,
  endDate: string,
): Promise<PayrollReport> {
  return apiRequestForBranch<PayrollReport>(
    `/api/business/finance/payroll-report?${reportQuery(startDate, endDate)}`,
    branchId,
  );
}
