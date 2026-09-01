import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  BarChart3,
  CalendarDays,
  CircleDollarSign,
  Download,
  LoaderCircle,
  LockKeyhole,
  LockOpen,
  Save,
  Settings2,
  TrendingDown,
  TrendingUp,
  UsersRound,
} from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import type {
  EmployeeSalaryConfig,
  EmployeeSalaryRateCreateReq,
  FinancialPeriodChangeRequest,
  FinancialPeriodState,
  OperatingFinancialLine,
  OperatingFinancialReport,
  PayrollLine,
  PayrollReport,
  PermissionCode,
  ReportExportKind,
} from "../../api/generated/contracts";
import { friendlyApiError, type DownloadedFile } from "../../shared/api/client";
import { roleLabel } from "../../shared/lib/format";
import { useAuth } from "../auth/AuthProvider";
import { useOperationsScope } from "../operations/OperationsScopeProvider";
import {
  createEmployeeSalaryRate,
  downloadFinancialReport,
  changeFinancialPeriodForBranch,
  financeQueryKeys,
  getOperatingReportForBranch,
  getPayrollReportForBranch,
  listFinancialPeriodsForBranch,
  listSalaryConfigurations,
} from "./api";

type ReportTab = "financial" | "payroll" | "salary";
type ScopeMode = "tenant" | "active_branch";

function monthLabel(value: string): string {
  const [year, month] = value.split("-");
  return `Tháng ${Number(month)}/${year}`;
}

function FinancialPeriodDialog({
  period,
  pending,
  error,
  onClose,
  onSubmit,
}: {
  period: FinancialPeriodState;
  pending: boolean;
  error: Error | null;
  onClose: () => void;
  onSubmit: (request: FinancialPeriodChangeRequest) => void;
}): React.JSX.Element {
  const targetStatus: FinancialPeriodChangeRequest["status"] = period.status === "open" ? "closed" : "open";
  const [reason, setReason] = useState<string>("");
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/50 p-4">
      <form
        className="w-full max-w-lg rounded-2xl bg-white p-6 shadow-2xl"
        onSubmit={(event: FormEvent<HTMLFormElement>): void => {
          event.preventDefault();
          onSubmit({
            period_start: period.period_start,
            status: targetStatus,
            expected_revision_number: period.revision_number,
            reason: reason.trim(),
          });
        }}
      >
        <div className="flex items-start gap-4">
          <div className={`grid size-11 shrink-0 place-items-center rounded-xl ${targetStatus === "closed" ? "bg-red-50 text-red-700" : "bg-emerald-50 text-emerald-700"}`}>
            {targetStatus === "closed" ? <LockKeyhole className="size-5" /> : <LockOpen className="size-5" />}
          </div>
          <div>
            <h2 className="text-xl font-black text-slate-950">
              {targetStatus === "closed" ? "Khóa" : "Mở lại"} {monthLabel(period.period_start)}
            </h2>
            <p className="mt-1 text-sm text-slate-500">
              {targetStatus === "closed"
                ? "Hệ thống sẽ chốt thưởng theo lợi nhuận, chốt lương, tự động hoàn toàn bộ chi hộ và thu hồi toàn bộ tạm ứng còn lại có ngày tính lương trong tháng. Các khoản đã chốt được lưu bất biến."
                : "Mở lại cho phép ghi nhận điều chỉnh mới nhưng không xóa hoặc đảo các khoản đã thanh toán khi khóa trước đó."}
            </p>
          </div>
        </div>
        <label className="mt-5 block text-sm font-semibold text-slate-700">
          Lý do
          <textarea className="mt-2 min-h-28 w-full rounded-xl border-slate-300" maxLength={500} minLength={3} onChange={(event): void => setReason(event.target.value)} required value={reason} />
        </label>
        {error ? <p className="mt-3 text-sm font-semibold text-red-700">{friendlyApiError(error, "Không thể khóa kỳ. Hãy xử lý giờ làm bị trùng nguồn, dữ liệu chưa hoàn tất hoặc thay đổi đồng thời rồi thử lại.")}</p> : null}
        <div className="mt-5 flex justify-end gap-3">
          <button className="action-secondary" disabled={pending} onClick={onClose} type="button">Hủy</button>
          <button className="action-primary" disabled={pending} type="submit">
            {pending ? <LoaderCircle className="size-4 animate-spin" /> : targetStatus === "closed" ? <LockKeyhole className="size-4" /> : <LockOpen className="size-4" />}
            {targetStatus === "closed" ? "Khóa kỳ lương" : "Mở lại kỳ"}
          </button>
        </div>
      </form>
    </div>
  );
}

function localDate(date: Date): string {
  const offset: number = date.getTimezoneOffset();
  return new Date(date.getTime() - offset * 60_000).toISOString().slice(0, 10);
}

function currentMonthRange(): { start: string; end: string } {
  const now = new Date();
  return {
    start: localDate(new Date(now.getFullYear(), now.getMonth(), 1)),
    end: localDate(new Date(now.getFullYear(), now.getMonth() + 1, 0)),
  };
}

function monthRange(month: string): { start: string; end: string } | null {
  if (!/^\d{4}-\d{2}$/.test(month)) return null;
  const [yearText, monthText] = month.split("-");
  const year: number = Number(yearText);
  const monthIndex: number = Number(monthText) - 1;
  if (!Number.isInteger(year) || monthIndex < 0 || monthIndex > 11) return null;
  return {
    start: localDate(new Date(year, monthIndex, 1)),
    end: localDate(new Date(year, monthIndex + 1, 0)),
  };
}

function selectedMonthForRange(start: string, end: string): string {
  const month: string = start.slice(0, 7);
  const range: { start: string; end: string } | null = monthRange(month);
  return range?.start === start && range.end === end ? month : "";
}

function scaledAmount(value: string): bigint {
  const negative: boolean = value.startsWith("-");
  const normalized: string = negative ? value.slice(1) : value;
  const [whole = "0", fraction = ""] = normalized.split(".");
  const amount: bigint = BigInt(whole || "0") * 10_000n
    + BigInt(fraction.padEnd(4, "0").slice(0, 4) || "0");
  return negative ? -amount : amount;
}

function decimalAmount(value: bigint): string {
  const negative: boolean = value < 0n;
  const absolute: bigint = negative ? -value : value;
  const whole: string = (absolute / 10_000n).toString();
  const fraction: string = (absolute % 10_000n).toString().padStart(4, "0");
  return `${negative ? "-" : ""}${whole}.${fraction}`;
}

function formatMoney(value: string, currency: string): string {
  const amount: bigint = scaledAmount(value);
  const negative: boolean = amount < 0n;
  const absolute: bigint = negative ? -amount : amount;
  const whole: string = (absolute / 10_000n).toString().replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  const fraction: string = (absolute % 10_000n).toString().padStart(4, "0").replace(/0+$/, "");
  return `${negative ? "-" : ""}${whole}${fraction ? `,${fraction}` : ""} ${currency}`;
}

function sumByCurrency<T>(rows: T[], amount: (row: T) => string, currency: (row: T) => string): string {
  const totals = new Map<string, bigint>();
  for (const row of rows) {
    const code: string = currency(row);
    totals.set(code, (totals.get(code) ?? 0n) + scaledAmount(amount(row)));
  }
  return totals.size === 0
    ? "0 VND"
    : [...totals.entries()].map(([code, total]): string => formatMoney(decimalAmount(total), code)).join(" · ");
}

function aggregateFinancialLines(reports: OperatingFinancialReport[]): OperatingFinancialLine[] {
  const fields: Array<keyof Omit<OperatingFinancialLine, "currency">> = [
    "staffing_revenue",
    "staffing_worker_cost",
    "coordination_salary_cost",
    "approved_business_expense",
    "profit_share_cost",
    "operating_cost",
    "operating_profit",
    "reimbursed_cash",
    "salary_advance_disbursed",
    "salary_advance_recovered",
    "outstanding_expense_reimbursement",
    "outstanding_salary_advance",
  ];
  const totals = new Map<string, Record<string, bigint>>();
  for (const line of reports.flatMap((report): OperatingFinancialLine[] => report.lines)) {
    const current: Record<string, bigint> = totals.get(line.currency) ?? {};
    for (const field of fields) {
      current[field] = (current[field] ?? 0n) + scaledAmount(line[field]);
    }
    totals.set(line.currency, current);
  }
  return [...totals.entries()].map(([currency, values]): OperatingFinancialLine => ({
    currency,
    staffing_revenue: decimalAmount(values.staffing_revenue ?? 0n),
    staffing_worker_cost: decimalAmount(values.staffing_worker_cost ?? 0n),
    coordination_salary_cost: decimalAmount(values.coordination_salary_cost ?? 0n),
    approved_business_expense: decimalAmount(values.approved_business_expense ?? 0n),
    profit_share_cost: decimalAmount(values.profit_share_cost ?? 0n),
    operating_cost: decimalAmount(values.operating_cost ?? 0n),
    operating_profit: decimalAmount(values.operating_profit ?? 0n),
    reimbursed_cash: decimalAmount(values.reimbursed_cash ?? 0n),
    salary_advance_disbursed: decimalAmount(values.salary_advance_disbursed ?? 0n),
    salary_advance_recovered: decimalAmount(values.salary_advance_recovered ?? 0n),
    outstanding_expense_reimbursement: decimalAmount(values.outstanding_expense_reimbursement ?? 0n),
    outstanding_salary_advance: decimalAmount(values.outstanding_salary_advance ?? 0n),
  }));
}

function aggregatePayrollLines(reports: PayrollReport[]): PayrollLine[] {
  const totals = new Map<string, PayrollLine>();
  for (const line of reports.flatMap((report): PayrollLine[] => report.lines)) {
    const key: string = `${line.employee_id}:${line.currency}`;
    const current: PayrollLine | undefined = totals.get(key);
    if (!current) {
      totals.set(key, { ...line });
      continue;
    }
    totals.set(key, {
      ...current,
      staffing_worked_seconds: current.staffing_worked_seconds + line.staffing_worked_seconds,
      staffing_earnings: decimalAmount(scaledAmount(current.staffing_earnings) + scaledAmount(line.staffing_earnings)),
      prorated_monthly_salary: decimalAmount(scaledAmount(current.prorated_monthly_salary) + scaledAmount(line.prorated_monthly_salary)),
      profit_share_base: decimalAmount(scaledAmount(current.profit_share_base) + scaledAmount(line.profit_share_base)),
      profit_share_percent: scaledAmount(current.profit_share_percent) >= scaledAmount(line.profit_share_percent)
        ? current.profit_share_percent
        : line.profit_share_percent,
      profit_share_payment: decimalAmount(scaledAmount(current.profit_share_payment) + scaledAmount(line.profit_share_payment)),
      profit_share_locked: current.profit_share_locked && line.profit_share_locked,
      gross_pay: decimalAmount(scaledAmount(current.gross_pay) + scaledAmount(line.gross_pay)),
      recorded_expense_reimbursement: decimalAmount(scaledAmount(current.recorded_expense_reimbursement) + scaledAmount(line.recorded_expense_reimbursement)),
      suggested_expense_reimbursement: decimalAmount(scaledAmount(current.suggested_expense_reimbursement) + scaledAmount(line.suggested_expense_reimbursement)),
      recorded_advance_deduction: decimalAmount(scaledAmount(current.recorded_advance_deduction) + scaledAmount(line.recorded_advance_deduction)),
      outstanding_advance_due: decimalAmount(scaledAmount(current.outstanding_advance_due) + scaledAmount(line.outstanding_advance_due)),
      suggested_advance_deduction: decimalAmount(scaledAmount(current.suggested_advance_deduction) + scaledAmount(line.suggested_advance_deduction)),
      estimated_net_pay: decimalAmount(scaledAmount(current.estimated_net_pay) + scaledAmount(line.estimated_net_pay)),
      attendance_overlap_count: current.attendance_overlap_count + line.attendance_overlap_count,
    });
  }
  return [...totals.values()].sort((left: PayrollLine, right: PayrollLine): number =>
    left.employee_name.localeCompare(right.employee_name, "vi"));
}

function hours(seconds: number): string {
  return `${(seconds / 3_600).toLocaleString("vi-VN", { maximumFractionDigits: 2 })} giờ`;
}

function formatPercent(value: string): string {
  return `${Number(value).toLocaleString("vi-VN", { maximumFractionDigits: 4 })}%`;
}

function saveDownloadedFile(file: DownloadedFile): void {
  const url: string = URL.createObjectURL(file.blob);
  const link: HTMLAnchorElement = document.createElement("a");
  link.href = url;
  link.download = file.filename ?? "bao-cao.xlsx";
  document.body.append(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

export function PayrollAccountingPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const scope: ReturnType<typeof useOperationsScope> = useOperationsScope();
  const queryClient = useQueryClient();
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canReadFinancial: boolean = permissions.includes("finance.operating_reports.read");
  const canReadPayroll: boolean = permissions.includes("hr.payroll.read");
  const canExportFinancial: boolean = permissions.includes("finance.operating_reports.export");
  const canExportPayroll: boolean = permissions.includes("hr.payroll.export");
  const canReadSalary: boolean = permissions.includes("hr.salary_rates.read");
  const canManageSalary: boolean = permissions.includes("hr.salary_rates.manage");
  const canManagePeriods: boolean = permissions.includes("finance.periods.manage");
  const initialRange = useMemo(currentMonthRange, []);
  const [startDate, setStartDate] = useState<string>(initialRange.start);
  const [endDate, setEndDate] = useState<string>(initialRange.end);
  const [selectedMonth, setSelectedMonth] = useState<string>(initialRange.start.slice(0, 7));
  const [scopeMode, setScopeMode] = useState<ScopeMode>("active_branch");
  const [tab, setTab] = useState<ReportTab>(canReadFinancial ? "financial" : "payroll");
  const [salaryDraft, setSalaryDraft] = useState<EmployeeSalaryRateCreateReq>({
    employee_id: "",
    monthly_amount: "",
    currency: "VND",
    effective_from: localDate(new Date()),
  });
  const [feedback, setFeedback] = useState<string | null>(null);
  const [periodAction, setPeriodAction] = useState<FinancialPeriodState | null>(null);

  const activeBranchId: string | null = auth.profile?.active_branch_id ?? null;
  const reportBranchIds: string[] = scopeMode === "tenant"
    ? scope.branches.map((branch): string => branch.id)
    : activeBranchId ? [activeBranchId] : [];
  const reportScopeKey: string = reportBranchIds.join(",") || "none";
  const validRange: boolean = Boolean(startDate && endDate && startDate <= endDate);
  const activeTab: ReportTab = tab === "financial" && !canReadFinancial
    ? canReadPayroll ? "payroll" : "salary"
    : tab === "payroll" && !canReadPayroll
      ? canReadFinancial ? "financial" : "salary"
      : tab === "salary" && !canReadSalary
        ? canReadFinancial ? "financial" : "payroll"
        : tab;

  const financialQuery = useQuery({
    queryKey: [...financeQueryKeys.operatingReport, reportScopeKey, startDate, endDate],
    queryFn: (): Promise<OperatingFinancialReport[]> => Promise.all(
      reportBranchIds.map((branchId: string): Promise<OperatingFinancialReport> =>
        getOperatingReportForBranch(branchId, startDate, endDate)),
    ),
    enabled: canReadFinancial && validRange && reportBranchIds.length > 0,
  });
  const payrollQuery = useQuery({
    queryKey: [...financeQueryKeys.payrollReport, reportScopeKey, startDate, endDate],
    queryFn: (): Promise<PayrollReport[]> => Promise.all(
      reportBranchIds.map((branchId: string): Promise<PayrollReport> =>
        getPayrollReportForBranch(branchId, startDate, endDate)),
    ),
    enabled: canReadPayroll && validRange && reportBranchIds.length > 0,
  });
  const salaryQuery = useQuery({
    queryKey: [...financeQueryKeys.salaryConfigurations, activeBranchId],
    queryFn: listSalaryConfigurations,
    enabled: canReadSalary && activeBranchId !== null,
  });
  const periodsQuery = useQuery({
    queryKey: [...financeQueryKeys.financialPeriods, activeBranchId, startDate, endDate],
    queryFn: (): Promise<FinancialPeriodState[]> => listFinancialPeriodsForBranch(
      activeBranchId ?? "",
      startDate,
      endDate,
    ),
    enabled: canReadFinancial && validRange && activeBranchId !== null,
  });
  const salaryMutation = useMutation({
    mutationFn: createEmployeeSalaryRate,
    onSuccess: (record: EmployeeSalaryConfig): void => {
      void queryClient.invalidateQueries({ queryKey: financeQueryKeys.salaryConfigurations });
      void queryClient.invalidateQueries({ queryKey: financeQueryKeys.payrollReport });
      void queryClient.invalidateQueries({ queryKey: financeQueryKeys.operatingReport });
      setFeedback(`Đã tạo mức lương mới cho ${record.employee_name}.`);
      setSalaryDraft((current): EmployeeSalaryRateCreateReq => ({
        ...current,
        employee_id: "",
        monthly_amount: "",
      }));
    },
  });
  const periodMutation = useMutation<FinancialPeriodState, Error, FinancialPeriodChangeRequest>({
    mutationFn: (request: FinancialPeriodChangeRequest): Promise<FinancialPeriodState> =>
      changeFinancialPeriodForBranch(activeBranchId ?? "", request),
    onSuccess: (period: FinancialPeriodState): void => {
      void queryClient.invalidateQueries({ queryKey: financeQueryKeys.financialPeriods });
      void queryClient.invalidateQueries({ queryKey: financeQueryKeys.expenses });
      void queryClient.invalidateQueries({ queryKey: financeQueryKeys.salaryAdvances });
      void queryClient.invalidateQueries({ queryKey: financeQueryKeys.operatingReport });
      void queryClient.invalidateQueries({ queryKey: financeQueryKeys.payrollReport });
      setPeriodAction(null);
      setFeedback(`${monthLabel(period.period_start)} đã được ${period.status === "closed" ? "khóa; chi hộ và tạm ứng còn lại đã tự động tính vào lương" : "mở lại"}.`);
    },
  });
  const exportMutation = useMutation<DownloadedFile, Error, ReportExportKind>({
    mutationFn: (reportKind: ReportExportKind): Promise<DownloadedFile> => downloadFinancialReport({
      report_kind: reportKind,
      start_date: startDate,
      end_date: endDate,
      branch_ids: reportBranchIds,
    }),
    onSuccess: (file: DownloadedFile, reportKind: ReportExportKind): void => {
      saveDownloadedFile(file);
      setFeedback(reportKind === "payroll" ? "Đã tạo và tải bảng lương Excel." : "Đã tạo và tải báo cáo tài chính Excel.");
    },
  });

  const financialLines: OperatingFinancialLine[] = aggregateFinancialLines(financialQuery.data ?? []);
  const payrollLines: PayrollLine[] = aggregatePayrollLines(payrollQuery.data ?? []);
  const overlapCount: number = payrollLines.reduce(
    (total: number, line: PayrollLine): number => total + line.attendance_overlap_count,
    0,
  );

  if (!canReadFinancial && !canReadPayroll && !canReadSalary) {
    return <section className="panel p-8 text-center font-bold text-slate-900">Bạn không có quyền xem lương và báo cáo tài chính.</section>;
  }

  return (
    <section className="space-y-5">
      <div className="panel grid gap-4 p-5 sm:grid-cols-2 xl:grid-cols-[1fr_1fr_1fr_1.2fr]">
        <label className="text-sm font-semibold text-slate-700">
          Chọn nhanh theo tháng
          <input
            className="mt-2 min-h-11 w-full rounded-xl border-slate-300"
            onChange={(event): void => {
              const month: string = event.target.value;
              setSelectedMonth(month);
              const range: { start: string; end: string } | null = monthRange(month);
              if (range) {
                setStartDate(range.start);
                setEndDate(range.end);
              }
            }}
            type="month"
            value={selectedMonth}
          />
        </label>
        <label className="text-sm font-semibold text-slate-700">
          Từ ngày
          <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => { const value: string = event.target.value; setStartDate(value); setSelectedMonth(selectedMonthForRange(value, endDate)); }} type="date" value={startDate} />
        </label>
        <label className="text-sm font-semibold text-slate-700">
          Đến ngày
          <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => { const value: string = event.target.value; setEndDate(value); setSelectedMonth(selectedMonthForRange(startDate, value)); }} type="date" value={endDate} />
        </label>
        <label className="text-sm font-semibold text-slate-700">
          Phạm vi báo cáo
          <select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setScopeMode(event.target.value as ScopeMode)} value={scopeMode}>
            <option value="active_branch">Chi nhánh đang chọn</option>
            <option value="tenant">Toàn doanh nghiệp</option>
          </select>
        </label>
        {!validRange ? <p className="text-sm font-semibold text-red-600 sm:col-span-2 xl:col-span-4">Khoảng ngày báo cáo không hợp lệ.</p> : null}
        {activeTab !== "salary" && ((activeTab === "financial" && canExportFinancial) || (activeTab === "payroll" && canExportPayroll)) ? (
          <div className="flex flex-col gap-2 border-t border-slate-200 pt-4 sm:col-span-2 sm:flex-row sm:items-center sm:justify-between xl:col-span-4">
            <p className="text-sm text-slate-500">File Excel dùng đúng khoảng ngày và phạm vi chi nhánh đang chọn. Trạng thái kỳ và các cảnh báo được ghi trong file.</p>
            <button
              className="action-primary min-h-11 w-full shrink-0 sm:w-auto"
              disabled={
                exportMutation.isPending
                || !validRange
                || reportBranchIds.length === 0
                || (activeTab === "financial" ? financialQuery.isPending || financialQuery.isError : payrollQuery.isPending || payrollQuery.isError)
              }
              onClick={(): void => {
                if (activeTab === "payroll" && overlapCount > 0 && !window.confirm("Bảng lương còn khoảng làm việc trùng nguồn. Bạn vẫn muốn xuất file để kiểm tra?")) return;
                exportMutation.reset();
                exportMutation.mutate(activeTab === "payroll" ? "payroll" : "operating_financial");
              }}
              type="button"
            >
              {exportMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : <Download className="size-4" />}
              {activeTab === "payroll" ? "Xuất bảng lương Excel" : "Xuất báo cáo Excel"}
            </button>
          </div>
        ) : null}
        {exportMutation.error ? <p className="text-sm font-semibold text-red-700 sm:col-span-2 xl:col-span-4">{friendlyApiError(exportMutation.error, "Không thể tạo file Excel. Vui lòng thu hẹp khoảng ngày hoặc phạm vi chi nhánh rồi thử lại.")}</p> : null}
      </div>

      <div className="panel flex flex-wrap gap-2 p-2">
        {canReadFinancial ? <button className={`flex-1 rounded-xl px-4 py-3 text-sm font-bold ${activeTab === "financial" ? "bg-slate-950 text-white" : "text-slate-600 hover:bg-slate-100"}`} onClick={(): void => setTab("financial")} type="button"><BarChart3 className="mr-2 inline size-4" />Tài chính vận hành</button> : null}
        {canReadPayroll ? <button className={`flex-1 rounded-xl px-4 py-3 text-sm font-bold ${activeTab === "payroll" ? "bg-slate-950 text-white" : "text-slate-600 hover:bg-slate-100"}`} onClick={(): void => setTab("payroll")} type="button"><UsersRound className="mr-2 inline size-4" />Bảng lương</button> : null}
        {canReadSalary ? <button className={`flex-1 rounded-xl px-4 py-3 text-sm font-bold ${activeTab === "salary" ? "bg-slate-950 text-white" : "text-slate-600 hover:bg-slate-100"}`} onClick={(): void => setTab("salary")} type="button"><Settings2 className="mr-2 inline size-4" />Cấu hình lương tháng</button> : null}
      </div>

      {canReadFinancial && activeBranchId ? (
        <div className="panel p-5">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
            <div>
              <h2 className="font-black text-slate-950">Kỳ lương và tài chính · {scope.branches.find((branch): boolean => branch.id === activeBranchId)?.name ?? "Chi nhánh đang chọn"}</h2>
              <p className="mt-1 text-sm text-slate-500">Khi khóa kỳ, hệ thống chốt thưởng theo lợi nhuận và số tiền thực trả, đồng thời xử lý số dư chi hộ, tạm ứng có ngày tính lương trong tháng. Mở lại không xóa các khoản đã chốt.</p>
            </div>
            {scopeMode === "tenant" ? <span className="rounded-full bg-amber-50 px-3 py-1 text-xs font-bold text-amber-800">Chỉ áp dụng cho chi nhánh đang chọn</span> : null}
          </div>
          {periodsQuery.isPending ? <div className="mt-4 flex items-center gap-2 text-sm text-slate-500"><LoaderCircle className="size-4 animate-spin" />Đang tải trạng thái kỳ...</div>
            : periodsQuery.error ? <p className="mt-4 text-sm font-semibold text-red-700">{friendlyApiError(periodsQuery.error, "Không thể tải trạng thái kỳ tài chính.")}</p>
              : <div className="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-3">{(periodsQuery.data ?? []).map((period: FinancialPeriodState): React.JSX.Element => <article className="rounded-xl border border-slate-200 p-4" key={period.period_start}><div className="flex items-start justify-between gap-3"><div><p className="font-bold text-slate-950">{monthLabel(period.period_start)}</p><p className="mt-1 text-xs text-slate-500">Phiên bản {period.revision_number || "mặc định"}{period.actor_username ? ` · ${period.actor_username}` : ""}</p></div><span className={`rounded-full px-2 py-1 text-xs font-bold ${period.status === "closed" ? "bg-red-50 text-red-700" : "bg-emerald-50 text-emerald-700"}`}>{period.status === "closed" ? "Đã khóa" : "Đang mở"}</span></div>{period.reason ? <p className="mt-3 text-sm text-slate-600">{period.reason}</p> : null}{canManagePeriods ? <button className="action-secondary mt-4 w-full" onClick={(): void => { periodMutation.reset(); setPeriodAction(period); }} type="button">{period.status === "closed" ? <LockOpen className="size-4" /> : <LockKeyhole className="size-4" />}{period.status === "closed" ? "Mở lại kỳ" : "Khóa kỳ lương"}</button> : null}</article>)}</div>}
        </div>
      ) : null}

      {feedback ? <p className="rounded-xl bg-emerald-50 px-4 py-3 text-sm font-semibold text-emerald-800">{feedback}</p> : null}

      {activeTab === "financial" && canReadFinancial ? (
        financialQuery.isPending ? <div className="panel grid min-h-64 place-items-center"><LoaderCircle className="size-7 animate-spin text-blue-600" /></div>
          : financialQuery.error ? <div className="panel p-5 text-sm text-red-700">{friendlyApiError(financialQuery.error, "Không thể tính báo cáo tài chính.")}</div>
            : <>
              <div className="grid gap-4 lg:grid-cols-3">
                <div className="panel flex items-center gap-4 p-5"><div className="grid size-12 place-items-center rounded-2xl bg-blue-50 text-blue-700"><TrendingUp className="size-6" /></div><div><p className="text-sm font-semibold text-slate-500">Doanh thu đã đối soát</p><p className="mt-1 text-xl font-black text-slate-950">{sumByCurrency(financialLines, (line): string => line.staffing_revenue, (line): string => line.currency)}</p></div></div>
                <div className="panel flex items-center gap-4 p-5"><div className="grid size-12 place-items-center rounded-2xl bg-amber-50 text-amber-700"><TrendingDown className="size-6" /></div><div><p className="text-sm font-semibold text-slate-500">Chi phí vận hành (không gồm thưởng)</p><p className="mt-1 text-xl font-black text-slate-950">{sumByCurrency(financialLines, (line): string => line.operating_cost, (line): string => line.currency)}</p></div></div>
                <div className="panel flex items-center gap-4 p-5"><div className="grid size-12 place-items-center rounded-2xl bg-emerald-50 text-emerald-700"><CircleDollarSign className="size-6" /></div><div><p className="text-sm font-semibold text-slate-500">Lợi nhuận vận hành (căn cứ thưởng)</p><p className="mt-1 text-xl font-black text-slate-950">{sumByCurrency(financialLines, (line): string => line.operating_profit, (line): string => line.currency)}</p></div></div>
              </div>
              <div className="panel overflow-x-auto">
                <table className="min-w-full text-sm"><thead className="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500"><tr><th className="px-5 py-4">Chi nhánh</th><th className="px-5 py-4">Doanh thu</th><th className="px-5 py-4">Tiền công Staff</th><th className="px-5 py-4">Lương quản lý</th><th className="px-5 py-4">Chi phí khác</th><th className="px-5 py-4">Thưởng theo lợi nhuận (tách riêng)</th><th className="px-5 py-4">Lợi nhuận vận hành</th></tr></thead>
                  <tbody className="divide-y divide-slate-100">{(financialQuery.data ?? []).flatMap((report: OperatingFinancialReport): React.JSX.Element[] => report.lines.map((line: OperatingFinancialLine): React.JSX.Element => <tr key={`${report.branch_id}:${line.currency}`}><td className="px-5 py-4 font-bold text-slate-900">{report.branch_name}</td><td className="px-5 py-4">{formatMoney(line.staffing_revenue, line.currency)}</td><td className="px-5 py-4">{formatMoney(line.staffing_worker_cost, line.currency)}</td><td className="px-5 py-4">{formatMoney(line.coordination_salary_cost, line.currency)}</td><td className="px-5 py-4">{formatMoney(line.approved_business_expense, line.currency)}</td><td className="px-5 py-4 font-bold text-violet-700">{formatMoney(line.profit_share_cost, line.currency)}</td><td className={`px-5 py-4 font-black ${scaledAmount(line.operating_profit) < 0n ? "text-red-700" : "text-emerald-700"}`}>{formatMoney(line.operating_profit, line.currency)}</td></tr>))}</tbody>
                </table>
              </div>
              <div className="grid gap-4 lg:grid-cols-2">{financialLines.map((line: OperatingFinancialLine): React.JSX.Element => <div className="panel p-5" key={line.currency}><h3 className="font-black text-slate-950">Dòng tiền và số dư · {line.currency}</h3><dl className="mt-4 grid gap-3 text-sm sm:grid-cols-2"><div><dt className="text-slate-500">Đã hoàn chi hộ trong kỳ</dt><dd className="font-bold">{formatMoney(line.reimbursed_cash, line.currency)}</dd></div><div><dt className="text-slate-500">Còn phải hoàn cuối kỳ</dt><dd className="font-bold text-amber-700">{formatMoney(line.outstanding_expense_reimbursement, line.currency)}</dd></div><div><dt className="text-slate-500">Đã chi tạm ứng trong kỳ</dt><dd className="font-bold">{formatMoney(line.salary_advance_disbursed, line.currency)}</dd></div><div><dt className="text-slate-500">Tạm ứng còn phải thu cuối kỳ</dt><dd className="font-bold text-violet-700">{formatMoney(line.outstanding_salary_advance, line.currency)}</dd></div></dl><p className="mt-4 text-xs text-slate-500">Hoàn chi hộ và tạm ứng là dòng tiền hoặc thanh toán công nợ, không được tính lại thành chi phí.</p></div>)}</div>
            </>
      ) : null}

      {activeTab === "payroll" && canReadPayroll ? (
        payrollQuery.isPending ? <div className="panel grid min-h-64 place-items-center"><LoaderCircle className="size-7 animate-spin text-violet-600" /></div>
          : payrollQuery.error ? <div className="panel p-5 text-sm text-red-700">{friendlyApiError(payrollQuery.error, "Không thể tính bảng lương.")}</div>
            : <>
              {overlapCount > 0 ? <div className="flex gap-3 rounded-2xl border border-red-200 bg-red-50 p-4 text-sm text-red-800"><AlertTriangle className="mt-0.5 size-5 shrink-0" /><div><p className="font-black">Có {overlapCount} khoảng làm việc bị trùng nguồn</p><p className="mt-1">Cần xử lý phần giao nhau giữa công việc khách hàng và chấm công nội bộ trước khi dùng bảng này để trả lương.</p></div></div> : null}
              <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4"><div className="panel p-5"><p className="text-sm font-semibold text-slate-500">Tổng lương gộp</p><p className="mt-1 text-xl font-black">{sumByCurrency(payrollLines, (line): string => line.gross_pay, (line): string => line.currency)}</p></div><div className="panel p-5"><p className="text-sm font-semibold text-slate-500">Hoàn chi hộ</p><p className="mt-1 text-xl font-black text-blue-700">{sumByCurrency(payrollLines, (line): string => decimalAmount(scaledAmount(line.recorded_expense_reimbursement) + scaledAmount(line.suggested_expense_reimbursement)), (line): string => line.currency)}</p></div><div className="panel p-5"><p className="text-sm font-semibold text-slate-500">Khấu trừ tạm ứng</p><p className="mt-1 text-xl font-black text-violet-700">{sumByCurrency(payrollLines, (line): string => decimalAmount(scaledAmount(line.recorded_advance_deduction) + scaledAmount(line.suggested_advance_deduction)), (line): string => line.currency)}</p></div><div className="panel p-5"><p className="text-sm font-semibold text-slate-500">Thực trả</p><p className="mt-1 text-xl font-black text-emerald-700">{sumByCurrency(payrollLines, (line): string => line.estimated_net_pay, (line): string => line.currency)}</p></div></div>
              <div className="panel overflow-x-auto">
                <table className="min-w-full text-sm">
                  <thead className="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500">
                    <tr><th className="px-5 py-4">Nhân viên</th><th className="px-5 py-4">Nguồn lương</th><th className="px-5 py-4">Thưởng theo lợi nhuận</th><th className="px-5 py-4">Lương gộp</th><th className="px-5 py-4">Hoàn chi hộ</th><th className="px-5 py-4">Khấu trừ tạm ứng</th><th className="px-5 py-4">Thực trả</th></tr>
                  </thead>
                  <tbody className="divide-y divide-slate-100">
                    {payrollLines.map((line: PayrollLine): React.JSX.Element => (
                      <tr className={line.attendance_overlap_count > 0 ? "bg-red-50/60" : ""} key={`${line.employee_id}:${line.currency}`}>
                        <td className="px-5 py-4">
                          <p className="font-bold text-slate-950">{line.employee_name}</p>
                          <p className="mt-1 text-xs text-slate-500">{line.employee_code} · {roleLabel(line.role)} · {scopeMode === "tenant" ? "Toàn doanh nghiệp" : scope.branches.find((branch): boolean => branch.id === line.branch_id)?.name ?? "Chi nhánh"}</p>
                          {line.attendance_overlap_count > 0 ? <p className="mt-1 text-xs font-bold text-red-700">Trùng {line.attendance_overlap_count} khoảng chấm công</p> : null}
                        </td>
                        <td className="px-5 py-4"><p>Tiền công: {formatMoney(line.staffing_earnings, line.currency)} · {hours(line.staffing_worked_seconds)}</p><p className="mt-1">Lương tháng phân bổ: {formatMoney(line.prorated_monthly_salary, line.currency)}</p></td>
                        <td className="px-5 py-4">
                          <p className="font-black text-violet-700">{formatMoney(line.profit_share_payment, line.currency)}</p>
                          <p className="mt-1 text-xs text-slate-500">{formatPercent(line.profit_share_percent)} của {formatMoney(line.profit_share_base, line.currency)}</p>
                          {scaledAmount(line.profit_share_percent) > 0n ? <span className={`mt-2 inline-flex rounded-full px-2 py-1 text-[11px] font-bold ${line.profit_share_locked ? "bg-slate-200 text-slate-700" : "bg-amber-50 text-amber-800"}`}>{line.profit_share_locked ? "Đã khóa" : "Tạm tính"}</span> : null}
                        </td>
                        <td className="px-5 py-4 font-bold">{formatMoney(line.gross_pay, line.currency)}</td>
                        <td className="px-5 py-4"><p>Đã tính: {formatMoney(line.recorded_expense_reimbursement, line.currency)}</p><p className="mt-1 font-bold text-blue-700">Khi khóa: {formatMoney(line.suggested_expense_reimbursement, line.currency)}</p></td>
                        <td className="px-5 py-4"><p>Đã tính: {formatMoney(line.recorded_advance_deduction, line.currency)}</p><p className="mt-1 font-bold text-violet-700">Khi khóa: {formatMoney(line.suggested_advance_deduction, line.currency)}</p></td>
                        <td className="px-5 py-4 font-black text-emerald-700">{formatMoney(line.estimated_net_pay, line.currency)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
      ) : null}

      {activeTab === "salary" && canReadSalary ? (
        <div className="space-y-5">
          {canManageSalary ? <form className="panel grid gap-4 p-5 lg:grid-cols-[1.5fr_1fr_0.6fr_1fr_auto] lg:items-end" onSubmit={(event: FormEvent<HTMLFormElement>): void => { event.preventDefault(); salaryMutation.mutate(salaryDraft); }}><label className="text-sm font-semibold text-slate-700">Nhân viên quản lý<select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setSalaryDraft((current): EmployeeSalaryRateCreateReq => ({ ...current, employee_id: event.target.value }))} required value={salaryDraft.employee_id}><option value="">Chọn nhân viên</option>{(salaryQuery.data ?? []).map((item: EmployeeSalaryConfig): React.JSX.Element => <option key={item.employee_id} value={item.employee_id}>{item.employee_name} · {roleLabel(item.role)}</option>)}</select></label><label className="text-sm font-semibold text-slate-700">Lương tháng<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" inputMode="decimal" onChange={(event): void => setSalaryDraft((current): EmployeeSalaryRateCreateReq => ({ ...current, monthly_amount: event.target.value }))} required value={salaryDraft.monthly_amount} /></label><label className="text-sm font-semibold text-slate-700">Tiền tệ<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300 uppercase" maxLength={3} onChange={(event): void => setSalaryDraft((current): EmployeeSalaryRateCreateReq => ({ ...current, currency: event.target.value.toUpperCase() }))} required value={salaryDraft.currency} /></label><label className="text-sm font-semibold text-slate-700">Hiệu lực từ<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" min={localDate(new Date())} onChange={(event): void => setSalaryDraft((current): EmployeeSalaryRateCreateReq => ({ ...current, effective_from: event.target.value }))} required type="date" value={salaryDraft.effective_from} /></label><button className="action-primary min-h-11" disabled={salaryMutation.isPending} type="submit">{salaryMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : <Save className="size-4" />}Lưu mức lương</button>{salaryMutation.error ? <p className="text-sm font-semibold text-red-700 lg:col-span-5">{friendlyApiError(salaryMutation.error, "Không thể lưu mức lương. Ngày hiệu lực có thể đã bị trùng.")}</p> : null}</form> : null}
          <div className="panel overflow-hidden"><div className="border-b border-slate-200 px-5 py-4"><h2 className="font-black text-slate-950">Lương tháng tại chi nhánh đang chọn</h2><p className="mt-1 text-sm text-slate-500">Mỗi thay đổi tạo một phiên bản mới; báo cáo tự phân bổ theo số ngày của từng tháng.</p></div>{salaryQuery.isPending ? <div className="grid min-h-48 place-items-center"><LoaderCircle className="size-6 animate-spin" /></div> : salaryQuery.error ? <p className="m-5 text-sm text-red-700">{friendlyApiError(salaryQuery.error, "Không thể tải cấu hình lương.")}</p> : <div className="divide-y divide-slate-100">{(salaryQuery.data ?? []).map((item: EmployeeSalaryConfig): React.JSX.Element => <article className="flex flex-col gap-3 p-5 sm:flex-row sm:items-center sm:justify-between" key={item.employee_id}><div><p className="font-bold text-slate-950">{item.employee_name}</p><p className="mt-1 text-sm text-slate-500">{item.employee_code} · {roleLabel(item.role)}</p></div><div className="sm:text-right">{item.monthly_amount && item.currency ? <><p className="font-black text-slate-950">{formatMoney(item.monthly_amount, item.currency)} / tháng</p><p className="mt-1 text-xs text-slate-500"><CalendarDays className="mr-1 inline size-3.5" />Hiệu lực {item.effective_from}{item.effective_to ? ` đến ${item.effective_to}` : " trở đi"}</p></> : <p className="font-semibold text-amber-700">Chưa cấu hình lương tháng</p>}</div></article>)}</div>}</div>
        </div>
      ) : null}
      {periodAction ? <FinancialPeriodDialog error={periodMutation.error} onClose={(): void => setPeriodAction(null)} onSubmit={(request: FinancialPeriodChangeRequest): void => periodMutation.mutate(request)} pending={periodMutation.isPending} period={periodAction} /> : null}
    </section>
  );
}
