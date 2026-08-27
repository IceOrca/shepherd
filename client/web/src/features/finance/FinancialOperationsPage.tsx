import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Banknote,
  Check,
  CircleDollarSign,
  LoaderCircle,
  Plus,
  Receipt,
  RefreshCw,
  Search,
  Wallet,
  X,
  XCircle,
} from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import type {
  Customer,
  Employee,
  ExpenseCategory,
  ExpenseClaim,
  ExpenseClaimCreateRequest,
  ExpenseClaimStatus,
  PermissionCode,
  SalaryAdvance,
  SalaryAdvanceCreateRequest,
  SalaryAdvanceRecoverySource,
  SalaryAdvanceStatus,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { useAuth } from "../auth/AuthProvider";
import { listCustomers, listEmployees, operationsQueryKeys } from "../operations/api";
import {
  approveExpense,
  approveSalaryAdvance,
  createExpense,
  createSalaryAdvance,
  disburseSalaryAdvance,
  financeQueryKeys,
  getOwnEmployee,
  listExpenseCategories,
  listExpenses,
  listSalaryAdvances,
  recoverSalaryAdvance,
  reimburseExpense,
  rejectExpense,
  rejectSalaryAdvance,
} from "./api";

type FinanceTab = "expenses" | "advances";
type ExpenseFilter = "all" | ExpenseClaimStatus;
type AdvanceFilter = "all" | SalaryAdvanceStatus;

type FinancialAction =
  | { kind: "expense_approve"; record: ExpenseClaim }
  | { kind: "expense_reject"; record: ExpenseClaim }
  | { kind: "expense_reimburse"; record: ExpenseClaim }
  | { kind: "advance_approve"; record: SalaryAdvance }
  | { kind: "advance_reject"; record: SalaryAdvance }
  | { kind: "advance_disburse"; record: SalaryAdvance }
  | { kind: "advance_recover"; record: SalaryAdvance };

interface ActionPayload {
  action: FinancialAction;
  amount: string;
  reason: string;
  reference: string;
  recoverySource: SalaryAdvanceRecoverySource;
}

function todayInput(): string {
  const today: Date = new Date();
  const offset: number = today.getTimezoneOffset();
  return new Date(today.getTime() - offset * 60_000).toISOString().slice(0, 10);
}

const emptyExpenseDraft: ExpenseClaimCreateRequest = {
  category_id: "",
  funding_source: "company_funds",
  paid_by_employee_id: null,
  customer_id: null,
  urgent_work_report_id: null,
  staffing_assignment_id: null,
  incurred_on: todayInput(),
  description: "",
  evidence_reference: null,
  claimed_amount: "",
  currency: "VND",
};

const emptyAdvanceDraft: SalaryAdvanceCreateRequest = {
  employee_id: "",
  requested_amount: "",
  currency: "VND",
  reason: "",
  recovery_due_on: null,
};

function scaledAmount(value: string): bigint {
  const [whole = "0", fraction = ""] = value.split(".");
  const normalizedFraction: string = fraction.padEnd(4, "0").slice(0, 4);
  return BigInt(whole || "0") * 10_000n + BigInt(normalizedFraction || "0");
}

function formatScaled(value: bigint, currency: string): string {
  const whole: string = (value / 10_000n).toString().replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  const fraction: string = (value % 10_000n).toString().padStart(4, "0").replace(/0+$/, "");
  return `${whole}${fraction ? `,${fraction}` : ""} ${currency}`;
}

function formatMoney(value: string, currency: string): string {
  return formatScaled(scaledAmount(value), currency);
}

function moneySummary<T>(rows: T[], amount: (row: T) => string, currency: (row: T) => string): string {
  const totals: Map<string, bigint> = new Map<string, bigint>();
  for (const row of rows) {
    const code: string = currency(row);
    totals.set(code, (totals.get(code) ?? 0n) + scaledAmount(amount(row)));
  }
  if (totals.size === 0) {
    return "0 VND";
  }
  return [...totals.entries()].map(([code, total]: [string, bigint]): string => formatScaled(total, code)).join(" · ");
}

function expenseStatusLabel(status: ExpenseClaimStatus): string {
  switch (status) {
    case "submitted": return "Chờ duyệt";
    case "approved": return "Đã duyệt";
    case "rejected": return "Từ chối";
    case "cancelled": return "Đã hủy";
  }
}

function advanceStatusLabel(status: SalaryAdvanceStatus): string {
  switch (status) {
    case "requested": return "Chờ duyệt";
    case "approved": return "Chờ chi tiền";
    case "disbursed": return "Đang thu hồi";
    case "recovered": return "Đã thu hồi đủ";
    case "rejected": return "Từ chối";
    case "cancelled": return "Đã hủy";
  }
}

function statusClass(status: ExpenseClaimStatus | SalaryAdvanceStatus): string {
  if (status === "approved" || status === "recovered") return "bg-emerald-50 text-emerald-700";
  if (status === "submitted" || status === "requested" || status === "disbursed") return "bg-amber-50 text-amber-700";
  return "bg-slate-100 text-slate-600";
}

function displayDate(value: string): string {
  return new Intl.DateTimeFormat("vi-VN", { day: "2-digit", month: "2-digit", year: "numeric" }).format(
    new Date(`${value}T00:00:00`),
  );
}

function ActionDialog({
  action,
  pending,
  error,
  onClose,
  onSubmit,
}: {
  action: FinancialAction;
  pending: boolean;
  error: Error | null;
  onClose: () => void;
  onSubmit: (payload: Omit<ActionPayload, "action">) => void;
}): React.JSX.Element {
  const approving: boolean = action.kind === "expense_approve" || action.kind === "advance_approve";
  const rejecting: boolean = action.kind === "expense_reject" || action.kind === "advance_reject";
  const reimbursing: boolean = action.kind === "expense_reimburse";
  const recovering: boolean = action.kind === "advance_recover";
  const recordAmount: string = "claimed_amount" in action.record
    ? action.record.claimed_amount
    : action.record.approved_amount ?? action.record.requested_amount;
  const defaultSettlement: string = "outstanding_reimbursement" in action.record
    ? action.record.outstanding_reimbursement
    : action.record.outstanding_amount;
  const [amount, setAmount] = useState<string>(approving ? recordAmount : defaultSettlement);
  const [reason, setReason] = useState<string>("");
  const [reference, setReference] = useState<string>("");
  const [recoverySource, setRecoverySource] = useState<SalaryAdvanceRecoverySource>("manual_repayment");
  const title: string = approving
    ? "Duyệt số tiền"
    : rejecting
      ? "Từ chối yêu cầu"
      : reimbursing
        ? "Ghi nhận hoàn trả chi phí"
        : action.kind === "advance_disburse"
          ? "Ghi nhận chi tạm ứng"
          : "Ghi nhận thu hồi tạm ứng";

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/50 p-4">
      <form
        className="w-full max-w-lg rounded-2xl bg-white p-6 shadow-2xl"
        onSubmit={(event: FormEvent<HTMLFormElement>): void => {
          event.preventDefault();
          onSubmit({ amount, reason: reason.trim(), reference: reference.trim(), recoverySource });
        }}
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 className="text-xl font-black text-slate-950">{title}</h2>
            <p className="mt-1 text-sm text-slate-500">
              {"description" in action.record ? action.record.description : `${action.record.employee_name} · ${action.record.reason}`}
            </p>
          </div>
          <button aria-label="Đóng" className="grid size-9 place-items-center rounded-lg hover:bg-slate-100" onClick={onClose} type="button">
            <X className="size-5" />
          </button>
        </div>
        <div className="mt-6 grid gap-4">
          {(approving || reimbursing || recovering) ? (
            <label className="text-sm font-semibold text-slate-700">
              Số tiền
              <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" inputMode="decimal" onChange={(event): void => setAmount(event.target.value)} required value={amount} />
            </label>
          ) : null}
          {rejecting || approving ? (
            <label className="text-sm font-semibold text-slate-700">
              {rejecting ? "Lý do từ chối" : "Lý do điều chỉnh (nếu số duyệt khác số đề nghị)"}
              <textarea className="mt-2 min-h-24 w-full rounded-xl border-slate-300" onChange={(event): void => setReason(event.target.value)} required={rejecting} value={reason} />
            </label>
          ) : null}
          {recovering ? (
            <label className="text-sm font-semibold text-slate-700">
              Cách thu hồi
              <select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setRecoverySource(event.target.value as SalaryAdvanceRecoverySource)} value={recoverySource}>
                <option value="manual_repayment">Nhân viên hoàn tiền trực tiếp</option>
                <option value="payroll_deduction">Khấu trừ khi trả lương</option>
              </select>
            </label>
          ) : null}
          {(reimbursing || recovering || action.kind === "advance_disburse") ? (
            <label className="text-sm font-semibold text-slate-700">
              Mã giao dịch hoặc chứng từ thanh toán
              <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={500} onChange={(event): void => setReference(event.target.value)} required value={reference} />
            </label>
          ) : null}
        </div>
        {error ? <p className="mt-4 rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(error, "Không thể hoàn tất thao tác tài chính.")}</p> : null}
        <div className="mt-6 flex justify-end gap-3">
          <button className="action-secondary" onClick={onClose} type="button">Đóng</button>
          <button className="action-primary" disabled={pending} type="submit">
            {pending ? <LoaderCircle className="size-4 animate-spin" /> : <Check className="size-4" />}
            Xác nhận
          </button>
        </div>
      </form>
    </div>
  );
}

export function FinancialOperationsPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const queryClient: ReturnType<typeof useQueryClient> = useQueryClient();
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canReadExpenses: boolean = permissions.includes("business.expenses.read") || permissions.includes("business.expenses.self.read");
  const canSubmitExpense: boolean = permissions.includes("business.expenses.submit");
  const canApproveExpense: boolean = permissions.includes("business.expenses.approve");
  const canSettleExpense: boolean = permissions.includes("business.expenses.settle");
  const canReadAdvances: boolean = permissions.includes("hr.salary_advances.read") || permissions.includes("hr.salary_advances.self.read");
  const canManageAdvances: boolean = permissions.includes("hr.salary_advances.manage") || permissions.includes("hr.salary_advances.self.request");
  const canApproveAdvances: boolean = permissions.includes("hr.salary_advances.approve");
  const canDisburseAdvances: boolean = permissions.includes("hr.salary_advances.disburse");
  const canRecoverAdvances: boolean = permissions.includes("hr.salary_advances.recover");
  const canReadEmployeeDirectory: boolean = permissions.includes("hr.employees.read");
  const canReadCustomers: boolean = permissions.includes("business.customers.read");
  const [tab, setTab] = useState<FinanceTab>("expenses");
  const [search, setSearch] = useState<string>("");
  const [expenseFilter, setExpenseFilter] = useState<ExpenseFilter>("all");
  const [advanceFilter, setAdvanceFilter] = useState<AdvanceFilter>("all");
  const [expenseFormOpen, setExpenseFormOpen] = useState<boolean>(false);
  const [advanceFormOpen, setAdvanceFormOpen] = useState<boolean>(false);
  const [expenseDraft, setExpenseDraft] = useState<ExpenseClaimCreateRequest>(emptyExpenseDraft);
  const [advanceDraft, setAdvanceDraft] = useState<SalaryAdvanceCreateRequest>(emptyAdvanceDraft);
  const [action, setAction] = useState<FinancialAction | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const activeTab: FinanceTab = tab === "expenses" && !canReadExpenses
    ? "advances"
    : tab === "advances" && !canReadAdvances
      ? "expenses"
      : tab;

  const categoriesQuery = useQuery({ queryKey: financeQueryKeys.expenseCategories, queryFn: listExpenseCategories, enabled: canReadExpenses });
  const expensesQuery = useQuery({ queryKey: financeQueryKeys.expenses, queryFn: listExpenses, enabled: canReadExpenses });
  const advancesQuery = useQuery({ queryKey: financeQueryKeys.salaryAdvances, queryFn: listSalaryAdvances, enabled: canReadAdvances });
  const employeesQuery = useQuery({
    queryKey: operationsQueryKeys.employees,
    queryFn: listEmployees,
    enabled: canReadEmployeeDirectory && (expenseFormOpen || advanceFormOpen),
  });
  const ownEmployeeQuery = useQuery({
    queryKey: ["finance", "own-employee"],
    queryFn: getOwnEmployee,
    enabled: !canReadEmployeeDirectory && (expenseFormOpen || advanceFormOpen),
  });
  const customersQuery = useQuery({
    queryKey: operationsQueryKeys.customers,
    queryFn: listCustomers,
    enabled: canReadCustomers && expenseFormOpen,
  });

  const employeeOptions: Employee[] = canReadEmployeeDirectory
    ? employeesQuery.data ?? []
    : ownEmployeeQuery.data ? [ownEmployeeQuery.data] : [];

  const visibleExpenses: ExpenseClaim[] = useMemo((): ExpenseClaim[] => {
    const query: string = search.trim().toLocaleLowerCase("vi");
    return (expensesQuery.data ?? []).filter((row: ExpenseClaim): boolean =>
      (expenseFilter === "all" || row.status === expenseFilter)
      && (!query || [row.description, row.category_name, row.paid_by_employee_name ?? "", row.submitted_by_username]
        .join(" ").toLocaleLowerCase("vi").includes(query)),
    );
  }, [expenseFilter, expensesQuery.data, search]);

  const visibleAdvances: SalaryAdvance[] = useMemo((): SalaryAdvance[] => {
    const query: string = search.trim().toLocaleLowerCase("vi");
    return (advancesQuery.data ?? []).filter((row: SalaryAdvance): boolean =>
      (advanceFilter === "all" || row.status === advanceFilter)
      && (!query || [row.employee_name, row.employee_code, row.reason, row.requested_by_username]
        .join(" ").toLocaleLowerCase("vi").includes(query)),
    );
  }, [advanceFilter, advancesQuery.data, search]);

  const approvedCosts: ExpenseClaim[] = (expensesQuery.data ?? []).filter((row: ExpenseClaim): boolean => row.status === "approved");
  const reimbursementBalances: ExpenseClaim[] = approvedCosts.filter((row: ExpenseClaim): boolean => scaledAmount(row.outstanding_reimbursement) > 0n);
  const advanceBalances: SalaryAdvance[] = (advancesQuery.data ?? []).filter((row: SalaryAdvance): boolean => scaledAmount(row.outstanding_amount) > 0n && (row.status === "disbursed" || row.status === "recovered"));

  const invalidateFinance = (): void => {
    void queryClient.invalidateQueries({ queryKey: financeQueryKeys.all });
  };

  const expenseCreateMutation = useMutation({
    mutationFn: createExpense,
    onSuccess: (record: ExpenseClaim): void => {
      invalidateFinance();
      setFeedback(`Đã ghi nhận chi phí ${formatMoney(record.claimed_amount, record.currency)}.`);
      setExpenseFormOpen(false);
      setExpenseDraft(emptyExpenseDraft);
    },
  });

  const advanceCreateMutation = useMutation({
    mutationFn: createSalaryAdvance,
    onSuccess: (record: SalaryAdvance): void => {
      invalidateFinance();
      setFeedback(`Đã tạo yêu cầu tạm ứng cho ${record.employee_name}.`);
      setAdvanceFormOpen(false);
      setAdvanceDraft(emptyAdvanceDraft);
    },
  });

  const actionMutation = useMutation<ExpenseClaim | SalaryAdvance, Error, ActionPayload>({
    mutationFn: async (payload: ActionPayload): Promise<ExpenseClaim | SalaryAdvance> => {
      switch (payload.action.kind) {
        case "expense_approve": return approveExpense(payload.action.record.id, { approved_amount: payload.amount, reason: payload.reason || null });
        case "expense_reject": return rejectExpense(payload.action.record.id, { reason: payload.reason });
        case "expense_reimburse": return reimburseExpense(payload.action.record.id, { amount: payload.amount, reference: payload.reference });
        case "advance_approve": return approveSalaryAdvance(payload.action.record.id, { approved_amount: payload.amount, reason: payload.reason || null });
        case "advance_reject": return rejectSalaryAdvance(payload.action.record.id, { reason: payload.reason });
        case "advance_disburse": return disburseSalaryAdvance(payload.action.record.id, { reference: payload.reference });
        case "advance_recover": return recoverSalaryAdvance(payload.action.record.id, { amount: payload.amount, source: payload.recoverySource, reference: payload.reference });
      }
    },
    onSuccess: (): void => {
      invalidateFinance();
      setFeedback("Đã cập nhật nghiệp vụ tài chính và lưu dấu vết kiểm toán.");
      setAction(null);
    },
  });

  if (!canReadExpenses && !canReadAdvances) {
    return <section className="panel p-8 text-center font-bold text-slate-900">Bạn không có quyền xem nghiệp vụ tài chính.</section>;
  }

  return (
    <section className="space-y-5">
      <div className="grid gap-4 lg:grid-cols-3">
        <div className="panel flex items-center gap-4 p-5">
          <div className="grid size-12 place-items-center rounded-2xl bg-blue-50 text-blue-700"><Receipt className="size-6" /></div>
          <div><p className="text-sm font-semibold text-slate-500">Chi phí đã duyệt</p><p className="mt-1 text-lg font-black text-slate-950">{moneySummary(approvedCosts, (row): string => row.approved_amount ?? "0", (row): string => row.currency)}</p></div>
        </div>
        <div className="panel flex items-center gap-4 p-5">
          <div className="grid size-12 place-items-center rounded-2xl bg-amber-50 text-amber-700"><Wallet className="size-6" /></div>
          <div><p className="text-sm font-semibold text-slate-500">Còn phải hoàn cho nhân viên</p><p className="mt-1 text-lg font-black text-slate-950">{moneySummary(reimbursementBalances, (row): string => row.outstanding_reimbursement, (row): string => row.currency)}</p></div>
        </div>
        <div className="panel flex items-center gap-4 p-5">
          <div className="grid size-12 place-items-center rounded-2xl bg-violet-50 text-violet-700"><CircleDollarSign className="size-6" /></div>
          <div><p className="text-sm font-semibold text-slate-500">Tạm ứng còn phải thu hồi</p><p className="mt-1 text-lg font-black text-slate-950">{moneySummary(advanceBalances, (row): string => row.outstanding_amount, (row): string => row.currency)}</p></div>
        </div>
      </div>

      {feedback ? <div className="rounded-xl bg-emerald-50 px-4 py-3 text-sm font-medium text-emerald-800">{feedback}</div> : null}

      <div className="panel p-2">
        <div className="flex gap-2">
          {canReadExpenses ? <button className={`flex-1 rounded-xl px-4 py-3 text-sm font-bold ${activeTab === "expenses" ? "bg-slate-950 text-white" : "text-slate-600 hover:bg-slate-100"}`} onClick={(): void => setTab("expenses")} type="button">Chi phí phát sinh</button> : null}
          {canReadAdvances ? <button className={`flex-1 rounded-xl px-4 py-3 text-sm font-bold ${activeTab === "advances" ? "bg-slate-950 text-white" : "text-slate-600 hover:bg-slate-100"}`} onClick={(): void => setTab("advances")} type="button">Tạm ứng lương</button> : null}
        </div>
      </div>

      <div className="panel flex flex-col gap-3 p-5 lg:flex-row lg:items-center">
        <label className="relative block flex-1">
          <Search className="absolute left-3 top-3 size-5 text-slate-400" />
          <input className="min-h-11 w-full rounded-xl border-slate-300 pl-10" onChange={(event): void => setSearch(event.target.value)} placeholder={activeTab === "expenses" ? "Tìm theo nội dung, loại chi phí hoặc người chi" : "Tìm theo nhân viên hoặc lý do tạm ứng"} type="search" value={search} />
        </label>
        {activeTab === "expenses" ? (
          <select className="min-h-11 rounded-xl border-slate-300" onChange={(event): void => setExpenseFilter(event.target.value as ExpenseFilter)} value={expenseFilter}>
            <option value="all">Tất cả trạng thái</option><option value="submitted">Chờ duyệt</option><option value="approved">Đã duyệt</option><option value="rejected">Từ chối</option>
          </select>
        ) : (
          <select className="min-h-11 rounded-xl border-slate-300" onChange={(event): void => setAdvanceFilter(event.target.value as AdvanceFilter)} value={advanceFilter}>
            <option value="all">Tất cả trạng thái</option><option value="requested">Chờ duyệt</option><option value="approved">Chờ chi tiền</option><option value="disbursed">Đang thu hồi</option><option value="recovered">Đã thu hồi đủ</option>
          </select>
        )}
        {activeTab === "expenses" && canSubmitExpense ? <button className="action-primary" onClick={(): void => { setExpenseDraft({ ...emptyExpenseDraft, category_id: categoriesQuery.data?.[0]?.id ?? "" }); setExpenseFormOpen(true); setFeedback(null); }} type="button"><Plus className="size-4" />Ghi nhận chi phí</button> : null}
        {activeTab === "advances" && canManageAdvances ? <button className="action-primary" onClick={(): void => { setAdvanceDraft(emptyAdvanceDraft); setAdvanceFormOpen(true); setFeedback(null); }} type="button"><Plus className="size-4" />Tạo tạm ứng</button> : null}
      </div>

      {activeTab === "expenses" ? (
        <div className="panel overflow-hidden">
          <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4"><div><h2 className="font-black text-slate-950">Chi phí theo chi nhánh đang chọn</h2><p className="mt-1 text-sm text-slate-500">{visibleExpenses.length} khoản · chi phí và hoàn trả được ghi riêng</p></div><button aria-label="Tải lại" className="grid size-10 place-items-center rounded-xl hover:bg-slate-100" onClick={(): void => { void expensesQuery.refetch(); }} type="button"><RefreshCw className={`size-4 ${expensesQuery.isFetching ? "animate-spin" : ""}`} /></button></div>
          {expensesQuery.isPending ? <div className="grid min-h-48 place-items-center"><LoaderCircle className="size-6 animate-spin text-blue-600" /></div> : expensesQuery.error ? <div className="m-5 rounded-xl bg-red-50 p-4 text-sm text-red-700">{friendlyApiError(expensesQuery.error, "Không thể tải chi phí.")}</div> : visibleExpenses.length === 0 ? <div className="p-10 text-center text-sm text-slate-500">Chưa có chi phí phù hợp.</div> : <div className="divide-y divide-slate-100">{visibleExpenses.map((row: ExpenseClaim): React.JSX.Element => (
            <article className="p-5" key={row.id}>
              <div className="flex flex-col gap-4 lg:flex-row lg:items-start">
                <div className={`grid size-11 shrink-0 place-items-center rounded-xl ${row.funding_source === "employee_personal" ? "bg-amber-50 text-amber-700" : "bg-blue-50 text-blue-700"}`}><Receipt className="size-5" /></div>
                <div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><h3 className="font-bold text-slate-950">{row.description}</h3><span className={`rounded-full px-2 py-1 text-xs font-bold ${statusClass(row.status)}`}>{expenseStatusLabel(row.status)}</span><span className="rounded-full bg-slate-100 px-2 py-1 text-xs font-semibold text-slate-600">{row.category_name}</span></div><p className="mt-2 text-sm text-slate-500">{displayDate(row.incurred_on)} · {row.funding_source === "employee_personal" ? `${row.paid_by_employee_name ?? "Nhân viên"} đã chi hộ` : "Chi bằng tiền công ty"} · ghi bởi {row.submitted_by_username}</p>{row.evidence_reference ? <p className="mt-1 text-xs text-slate-400">Chứng từ: {row.evidence_reference}</p> : null}{row.decision_reason ? <p className="mt-2 text-sm text-slate-600">Kết luận: {row.decision_reason}</p> : null}</div>
                <div className="shrink-0 text-left lg:text-right"><p className="text-lg font-black text-slate-950">{formatMoney(row.approved_amount ?? row.claimed_amount, row.currency)}</p><p className="mt-1 text-xs text-slate-500">{row.approved_amount ? `Đề nghị ${formatMoney(row.claimed_amount, row.currency)}` : "Số tiền đề nghị"}</p>{scaledAmount(row.outstanding_reimbursement) > 0n ? <p className="mt-2 text-sm font-bold text-amber-700">Còn hoàn {formatMoney(row.outstanding_reimbursement, row.currency)}</p> : null}</div>
              </div>
              <div className="mt-4 flex flex-wrap justify-end gap-2">{row.status === "submitted" && canApproveExpense ? <><button className="action-secondary" onClick={(): void => setAction({ kind: "expense_reject", record: row })} type="button"><XCircle className="size-4" />Từ chối</button><button className="action-primary" onClick={(): void => setAction({ kind: "expense_approve", record: row })} type="button"><Check className="size-4" />Duyệt</button></> : null}{row.status === "approved" && row.funding_source === "employee_personal" && scaledAmount(row.outstanding_reimbursement) > 0n && canSettleExpense ? <button className="action-primary" onClick={(): void => setAction({ kind: "expense_reimburse", record: row })} type="button"><Wallet className="size-4" />Hoàn trả</button> : null}</div>
            </article>
          ))}</div>}
        </div>
      ) : (
        <div className="panel overflow-hidden">
          <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4"><div><h2 className="font-black text-slate-950">Tạm ứng lương theo chi nhánh đang chọn</h2><p className="mt-1 text-sm text-slate-500">{visibleAdvances.length} khoản · tiền công gộp không bị sửa</p></div><button aria-label="Tải lại" className="grid size-10 place-items-center rounded-xl hover:bg-slate-100" onClick={(): void => { void advancesQuery.refetch(); }} type="button"><RefreshCw className={`size-4 ${advancesQuery.isFetching ? "animate-spin" : ""}`} /></button></div>
          {advancesQuery.isPending ? <div className="grid min-h-48 place-items-center"><LoaderCircle className="size-6 animate-spin text-violet-600" /></div> : advancesQuery.error ? <div className="m-5 rounded-xl bg-red-50 p-4 text-sm text-red-700">{friendlyApiError(advancesQuery.error, "Không thể tải tạm ứng lương.")}</div> : visibleAdvances.length === 0 ? <div className="p-10 text-center text-sm text-slate-500">Chưa có khoản tạm ứng phù hợp.</div> : <div className="divide-y divide-slate-100">{visibleAdvances.map((row: SalaryAdvance): React.JSX.Element => (
            <article className="p-5" key={row.id}>
              <div className="flex flex-col gap-4 lg:flex-row lg:items-start"><div className="grid size-11 shrink-0 place-items-center rounded-xl bg-violet-50 text-violet-700"><Banknote className="size-5" /></div><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><h3 className="font-bold text-slate-950">{row.employee_name}</h3><span className="rounded-full bg-slate-100 px-2 py-1 text-xs font-semibold text-slate-600">{row.employee_code}</span><span className={`rounded-full px-2 py-1 text-xs font-bold ${statusClass(row.status)}`}>{advanceStatusLabel(row.status)}</span></div><p className="mt-2 text-sm text-slate-600">{row.reason}</p><p className="mt-1 text-xs text-slate-400">Yêu cầu bởi {row.requested_by_username}{row.recovery_due_on ? ` · dự kiến thu hồi ${displayDate(row.recovery_due_on)}` : ""}</p>{row.decision_reason ? <p className="mt-2 text-sm text-slate-600">Kết luận: {row.decision_reason}</p> : null}{row.disbursement_reference ? <p className="mt-1 text-xs text-slate-400">Chứng từ chi: {row.disbursement_reference}</p> : null}</div><div className="shrink-0 text-left lg:text-right"><p className="text-lg font-black text-slate-950">{formatMoney(row.approved_amount ?? row.requested_amount, row.currency)}</p><p className="mt-1 text-xs text-slate-500">Đã thu hồi {formatMoney(row.recovered_amount, row.currency)}</p>{scaledAmount(row.outstanding_amount) > 0n && row.status === "disbursed" ? <p className="mt-2 text-sm font-bold text-violet-700">Còn thu {formatMoney(row.outstanding_amount, row.currency)}</p> : null}</div></div>
              <div className="mt-4 flex flex-wrap justify-end gap-2">{row.status === "requested" && canApproveAdvances ? <><button className="action-secondary" onClick={(): void => setAction({ kind: "advance_reject", record: row })} type="button"><XCircle className="size-4" />Từ chối</button><button className="action-primary" onClick={(): void => setAction({ kind: "advance_approve", record: row })} type="button"><Check className="size-4" />Duyệt</button></> : null}{row.status === "approved" && canDisburseAdvances ? <button className="action-primary" onClick={(): void => setAction({ kind: "advance_disburse", record: row })} type="button"><Banknote className="size-4" />Ghi nhận đã chi</button> : null}{row.status === "disbursed" && scaledAmount(row.outstanding_amount) > 0n && canRecoverAdvances ? <button className="action-primary" onClick={(): void => setAction({ kind: "advance_recover", record: row })} type="button"><CircleDollarSign className="size-4" />Thu hồi</button> : null}</div>
            </article>
          ))}</div>}
        </div>
      )}

      {expenseFormOpen ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/50 p-4"><form className="max-h-[92vh] w-full max-w-2xl overflow-y-auto rounded-2xl bg-white p-6 shadow-2xl" onSubmit={(event: FormEvent<HTMLFormElement>): void => { event.preventDefault(); expenseCreateMutation.mutate({ ...expenseDraft, description: expenseDraft.description.trim(), evidence_reference: expenseDraft.evidence_reference?.trim() || null, paid_by_employee_id: expenseDraft.funding_source === "employee_personal" ? expenseDraft.paid_by_employee_id : null, customer_id: expenseDraft.customer_id || null }); }}><div className="flex items-start justify-between gap-4"><div><h2 className="text-xl font-black text-slate-950">Ghi nhận chi phí phát sinh</h2><p className="mt-1 text-sm text-slate-500">Phân biệt rõ tiền công ty và tiền cá nhân đã chi hộ.</p></div><button aria-label="Đóng" className="grid size-9 place-items-center rounded-lg hover:bg-slate-100" onClick={(): void => setExpenseFormOpen(false)} type="button"><X className="size-5" /></button></div><div className="mt-6 grid gap-4 sm:grid-cols-2">
          <label className="text-sm font-semibold text-slate-700">Nguồn tiền<select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setExpenseDraft((current): ExpenseClaimCreateRequest => ({ ...current, funding_source: event.target.value as ExpenseClaimCreateRequest["funding_source"], paid_by_employee_id: null }))} value={expenseDraft.funding_source}><option value="company_funds">Tiền công ty</option><option value="employee_personal">Nhân viên chi hộ</option></select></label>
          <label className="text-sm font-semibold text-slate-700">Loại chi phí<select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setExpenseDraft((current): ExpenseClaimCreateRequest => ({ ...current, category_id: event.target.value }))} required value={expenseDraft.category_id}><option value="">Chọn loại chi phí</option>{(categoriesQuery.data ?? []).map((category: ExpenseCategory): React.JSX.Element => <option key={category.id} value={category.id}>{category.display_name}</option>)}</select></label>
          {expenseDraft.funding_source === "employee_personal" ? <label className="text-sm font-semibold text-slate-700">Người đã chi tiền<select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setExpenseDraft((current): ExpenseClaimCreateRequest => ({ ...current, paid_by_employee_id: event.target.value || null }))} required value={expenseDraft.paid_by_employee_id ?? ""}><option value="">Chọn nhân viên</option>{employeeOptions.filter((employee: Employee): boolean => employee.status === "active").map((employee: Employee): React.JSX.Element => <option key={employee.id} value={employee.id}>{employee.display_name} · {employee.employee_code}</option>)}</select></label> : null}
          <label className="text-sm font-semibold text-slate-700">Ngày phát sinh<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setExpenseDraft((current): ExpenseClaimCreateRequest => ({ ...current, incurred_on: event.target.value }))} required type="date" value={expenseDraft.incurred_on} /></label>
          <label className="text-sm font-semibold text-slate-700">Số tiền<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" inputMode="decimal" onChange={(event): void => setExpenseDraft((current): ExpenseClaimCreateRequest => ({ ...current, claimed_amount: event.target.value }))} required value={expenseDraft.claimed_amount} /></label>
          <label className="text-sm font-semibold text-slate-700">Tiền tệ<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300 uppercase" maxLength={3} onChange={(event): void => setExpenseDraft((current): ExpenseClaimCreateRequest => ({ ...current, currency: event.target.value.toUpperCase() }))} required value={expenseDraft.currency} /></label>
          {canReadCustomers ? <label className="text-sm font-semibold text-slate-700">Khách hàng liên quan (không bắt buộc)<select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setExpenseDraft((current): ExpenseClaimCreateRequest => ({ ...current, customer_id: event.target.value || null }))} value={expenseDraft.customer_id ?? ""}><option value="">Không gắn khách hàng</option>{(customersQuery.data ?? []).map((customer: Customer): React.JSX.Element => <option key={customer.id} value={customer.id}>{customer.name}</option>)}</select></label> : null}
          <label className="text-sm font-semibold text-slate-700 sm:col-span-2">Nội dung chi phí<textarea className="mt-2 min-h-24 w-full rounded-xl border-slate-300" maxLength={1000} onChange={(event): void => setExpenseDraft((current): ExpenseClaimCreateRequest => ({ ...current, description: event.target.value }))} required value={expenseDraft.description} /></label>
          <label className="text-sm font-semibold text-slate-700 sm:col-span-2">Số hóa đơn, ảnh hoặc tham chiếu chứng từ<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={500} onChange={(event): void => setExpenseDraft((current): ExpenseClaimCreateRequest => ({ ...current, evidence_reference: event.target.value }))} value={expenseDraft.evidence_reference ?? ""} /></label>
        </div>{expenseCreateMutation.error ? <p className="mt-4 rounded-xl bg-red-50 p-3 text-sm text-red-700">{friendlyApiError(expenseCreateMutation.error, "Không thể ghi nhận chi phí.")}</p> : null}<div className="mt-6 flex justify-end gap-3"><button className="action-secondary" onClick={(): void => setExpenseFormOpen(false)} type="button">Hủy</button><button className="action-primary" disabled={expenseCreateMutation.isPending} type="submit">{expenseCreateMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : <Plus className="size-4" />}Gửi duyệt</button></div></form></div>
      ) : null}

      {advanceFormOpen ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/50 p-4"><form className="w-full max-w-lg rounded-2xl bg-white p-6 shadow-2xl" onSubmit={(event: FormEvent<HTMLFormElement>): void => { event.preventDefault(); advanceCreateMutation.mutate({ ...advanceDraft, reason: advanceDraft.reason.trim(), recovery_due_on: advanceDraft.recovery_due_on || null }); }}><div className="flex items-start justify-between gap-4"><div><h2 className="text-xl font-black text-slate-950">Tạo yêu cầu tạm ứng lương</h2><p className="mt-1 text-sm text-slate-500">Khoản này chỉ giảm tiền thực trả khi được thu hồi, không giảm tiền công gộp.</p></div><button aria-label="Đóng" className="grid size-9 place-items-center rounded-lg hover:bg-slate-100" onClick={(): void => setAdvanceFormOpen(false)} type="button"><X className="size-5" /></button></div><div className="mt-6 grid gap-4">
          <label className="text-sm font-semibold text-slate-700">Nhân viên<select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setAdvanceDraft((current): SalaryAdvanceCreateRequest => ({ ...current, employee_id: event.target.value }))} required value={advanceDraft.employee_id}><option value="">Chọn nhân viên</option>{employeeOptions.filter((employee: Employee): boolean => employee.status === "active").map((employee: Employee): React.JSX.Element => <option key={employee.id} value={employee.id}>{employee.display_name} · {employee.employee_code}</option>)}</select></label>
          <div className="grid gap-4 sm:grid-cols-2"><label className="text-sm font-semibold text-slate-700">Số tiền<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" inputMode="decimal" onChange={(event): void => setAdvanceDraft((current): SalaryAdvanceCreateRequest => ({ ...current, requested_amount: event.target.value }))} required value={advanceDraft.requested_amount} /></label><label className="text-sm font-semibold text-slate-700">Tiền tệ<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300 uppercase" maxLength={3} onChange={(event): void => setAdvanceDraft((current): SalaryAdvanceCreateRequest => ({ ...current, currency: event.target.value.toUpperCase() }))} required value={advanceDraft.currency} /></label></div>
          <label className="text-sm font-semibold text-slate-700">Dự kiến thu hồi từ kỳ lương<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setAdvanceDraft((current): SalaryAdvanceCreateRequest => ({ ...current, recovery_due_on: event.target.value || null }))} type="date" value={advanceDraft.recovery_due_on ?? ""} /></label>
          <label className="text-sm font-semibold text-slate-700">Lý do tạm ứng<textarea className="mt-2 min-h-24 w-full rounded-xl border-slate-300" maxLength={500} onChange={(event): void => setAdvanceDraft((current): SalaryAdvanceCreateRequest => ({ ...current, reason: event.target.value }))} required value={advanceDraft.reason} /></label>
        </div>{advanceCreateMutation.error ? <p className="mt-4 rounded-xl bg-red-50 p-3 text-sm text-red-700">{friendlyApiError(advanceCreateMutation.error, "Không thể tạo tạm ứng.")}</p> : null}<div className="mt-6 flex justify-end gap-3"><button className="action-secondary" onClick={(): void => setAdvanceFormOpen(false)} type="button">Hủy</button><button className="action-primary" disabled={advanceCreateMutation.isPending} type="submit">{advanceCreateMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : <Plus className="size-4" />}Gửi duyệt</button></div></form></div>
      ) : null}

      {action ? <ActionDialog action={action} error={actionMutation.error} onClose={(): void => setAction(null)} onSubmit={(payload): void => actionMutation.mutate({ action, ...payload })} pending={actionMutation.isPending} /> : null}
    </section>
  );
}
