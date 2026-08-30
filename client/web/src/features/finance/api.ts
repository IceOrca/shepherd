import type {
  ExpenseCategory,
  ExpenseClaim,
  ExpenseClaimCreateRequest,
  ExpenseClaimRevision,
  ExpensePageResponse,
  ExpenseRevisionPageResponse,
  ExpenseCorrectionRequest,
  Employee,
  EmployeeSalaryConfiguration,
  EmployeeSalaryRateCreateRequest,
  FinancialDecisionRequest,
  FinancialPeriodChangeRequest,
  FinancialPeriodState,
  FinancialReportExportRequest,
  FinancialRejectionRequest,
  FinancialSettlementRequest,
  OperatingFinancialReport,
  PayrollReport,
  SalaryAdvance,
  SalaryAdvanceCorrectionRequest,
  SalaryAdvanceCreateRequest,
  SalaryAdvanceDisbursementRequest,
  SalaryAdvanceRecoveryRequest,
  SalaryAdvanceRevision,
  SalaryAdvancePageResponse,
  SalaryAdvanceRevisionPageResponse,
} from "../../api/generated/contracts";
import { apiFileRequest, apiRequest, apiRequestForBranch, type DownloadedFile } from "../../shared/api/client";

export const financeQueryKeys = {
  all: ["finance"] as const,
  expenseCategories: ["finance", "expense-categories"] as const,
  expenses: ["finance", "expenses"] as const,
  expenseRevisions: (expenseId: string) => ["finance", "expenses", expenseId, "revisions"] as const,
  salaryAdvances: ["finance", "salary-advances"] as const,
  salaryAdvanceRevisions: (advanceId: string) => ["finance", "salary-advances", advanceId, "revisions"] as const,
  salaryConfigurations: ["finance", "salary-configurations"] as const,
  operatingReport: ["finance", "operating-report"] as const,
  payrollReport: ["finance", "payroll-report"] as const,
  financialPeriods: ["finance", "periods"] as const,
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

function pageQuery(cursor: string | null, status?: string, search?: string): string {
  const parameters: URLSearchParams = new URLSearchParams();
  if (cursor !== null) parameters.set("cursor", cursor);
  if (status) parameters.set("status", status);
  if (search?.trim()) parameters.set("search", search.trim());
  const query: string = parameters.toString();
  return query ? `?${query}` : "";
}

export function listExpenses(
  cursor: string | null,
  status?: string,
  search?: string,
): Promise<ExpensePageResponse> {
  return apiRequest<ExpensePageResponse>(`/api/business/finance/expenses${pageQuery(cursor, status, search)}`);
}

export function createExpense(payload: ExpenseClaimCreateRequest): Promise<ExpenseClaim> {
  return apiRequest<ExpenseClaim>("/api/business/finance/expenses", {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function correctExpense(expenseId: string, payload: ExpenseCorrectionRequest): Promise<ExpenseClaim> {
  return apiRequest<ExpenseClaim>(`/api/business/finance/expenses/${expenseId}/correct`, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function listExpenseRevisions(
  expenseId: string,
  cursor: string | null,
): Promise<ExpenseRevisionPageResponse> {
  return apiRequest<ExpenseRevisionPageResponse>(
    `/api/business/finance/expenses/${expenseId}/revisions${pageQuery(cursor)}`,
  );
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

export function listSalaryAdvances(
  cursor: string | null,
  status?: string,
  search?: string,
): Promise<SalaryAdvancePageResponse> {
  return apiRequest<SalaryAdvancePageResponse>(
    `/api/business/finance/salary-advances${pageQuery(cursor, status, search)}`,
  );
}

export function createSalaryAdvance(payload: SalaryAdvanceCreateRequest): Promise<SalaryAdvance> {
  return apiRequest<SalaryAdvance>("/api/business/finance/salary-advances", {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function correctSalaryAdvance(
  advanceId: string,
  payload: SalaryAdvanceCorrectionRequest,
): Promise<SalaryAdvance> {
  return apiRequest<SalaryAdvance>(`/api/business/finance/salary-advances/${advanceId}/correct`, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
}

export function listSalaryAdvanceRevisions(
  advanceId: string,
  cursor: string | null,
): Promise<SalaryAdvanceRevisionPageResponse> {
  return apiRequest<SalaryAdvanceRevisionPageResponse>(
    `/api/business/finance/salary-advances/${advanceId}/revisions${pageQuery(cursor)}`,
  );
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

export function listFinancialPeriodsForBranch(
  branchId: string,
  startDate: string,
  endDate: string,
): Promise<FinancialPeriodState[]> {
  return apiRequestForBranch<FinancialPeriodState[]>(
    `/api/business/finance/periods?${reportQuery(startDate, endDate)}`,
    branchId,
  );
}

export function changeFinancialPeriodForBranch(
  branchId: string,
  payload: FinancialPeriodChangeRequest,
): Promise<FinancialPeriodState> {
  return apiRequestForBranch<FinancialPeriodState>("/api/business/finance/periods", branchId, {
    method: "POST",
    headers: mutationHeaders(),
    body: JSON.stringify(payload),
  });
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

export function downloadFinancialReport(payload: FinancialReportExportRequest): Promise<DownloadedFile> {
  return apiFileRequest("/api/business/finance/report-exports/xlsx", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}
