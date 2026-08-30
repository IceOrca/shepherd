import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { CheckCircle2, CircleAlert, GitCompareArrows, MapPin, RefreshCw, Save } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import type {
  StaffingJob,
  ManualRateOverrideRequest,
  PermissionCode,
  ReconciliationStatus,
  UrgentCustomerWorkRecord,
  UrgentReconciliationPageResponse,
  UrgentWorkCustomer,
  UrgentWorkReconciliation,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { formatDateTime, formatDuration } from "../../shared/lib/format";
import { useAuth } from "../auth/AuthProvider";
import {
  acceptUrgentStaffRecordForBranch,
  listJobsForBranch,
  listUrgentCustomersForBranch,
  listUrgentReconciliationsForBranch,
  operationsQueryKeys,
  reconcileUrgentWorkForBranch,
  saveUrgentCustomerWorkRecordForBranch,
} from "./api";
import { useOperationsScope } from "./OperationsScopeProvider";
import { ReconciliationModeSelector } from "./ReconciliationModeSelector";
import { ReconciliationPagination } from "./ReconciliationPagination";
import {
  createReconciliationScopeCursor,
  loadReconciliationScopePage,
  type ReconciliationScopeCursor,
  type ReconciliationScopePage,
} from "./reconciliationCursor";

interface EvidenceDraft {
  customerId: string;
  startedAt: string;
  endedAt: string;
  reference: string;
  notes: string;
}

interface FinalDraft {
  customerId: string;
  jobId: string;
  hours: string;
  reason: string;
  useManualRate: boolean;
  currency: string;
  billRate: string;
  workerRate: string;
  manualRateReason: string;
}

function localDateTime(value: string | null | undefined): string {
  if (!value) {
    return "";
  }
  const date: Date = new Date(value);
  const offsetMilliseconds: number = date.getTimezoneOffset() * 60_000;
  return new Date(date.getTime() - offsetMilliseconds).toISOString().slice(0, 16);
}

function statusLabel(status: ReconciliationStatus): string {
  const labels: Record<ReconciliationStatus, string> = {
    pending_staff: "Chờ nhân viên kết thúc",
    pending_customer: "Chờ dữ liệu khách hàng",
    matched: "Khớp hoàn toàn",
    discrepancy: "Chênh lệch",
    reconciled: "Đã chốt",
  };
  return labels[status];
}

function statusTone(status: ReconciliationStatus): string {
  if (status === "matched" || status === "reconciled") {
    return "bg-emerald-50 text-emerald-700";
  }
  if (status === "discrepancy") {
    return "bg-red-50 text-red-700";
  }
  return "bg-amber-50 text-amber-700";
}

function initialEvidence(item: UrgentWorkReconciliation): EvidenceDraft {
  const customerRecord: UrgentCustomerWorkRecord | null = item.customer_record;
  return {
    customerId: customerRecord?.confirmed_customer_id ?? "",
    startedAt: localDateTime(customerRecord?.confirmed_started_at),
    endedAt: localDateTime(customerRecord?.confirmed_ended_at),
    reference: customerRecord?.customer_reference ?? "",
    notes: customerRecord?.notes ?? "",
  };
}

function initialFinal(item: UrgentWorkReconciliation): FinalDraft {
  const seconds: number =
    item.final_worked_seconds ?? item.customer_record?.confirmed_worked_seconds ?? item.work.worked_seconds ?? 0;
  return {
    customerId:
      item.final_customer_id ??
      item.customer_record?.confirmed_customer_id ??
      item.work.claimed_customer_id,
    jobId: item.final_job_id ?? "",
    hours: seconds > 0 ? (seconds / 3600).toFixed(2) : "",
    reason: item.adjustment_reason ?? "",
    useManualRate: false,
    currency: "VND",
    billRate: "",
    workerRate: "",
    manualRateReason: "",
  };
}

function compareUrgentReconciliations(
  left: UrgentWorkReconciliation,
  right: UrgentWorkReconciliation,
): number {
  const leftActive: number = left.work.status === "active" ? 1 : 0;
  const rightActive: number = right.work.status === "active" ? 1 : 0;
  if (leftActive !== rightActive) {
    return rightActive - leftActive;
  }
  const timeDifference: number =
    new Date(right.work.started_at).getTime() - new Date(left.work.started_at).getTime();
  if (timeDifference !== 0) {
    return timeDifference;
  }
  if (left.work.report_id === right.work.report_id) {
    return 0;
  }
  return left.work.report_id < right.work.report_id ? 1 : -1;
}

export function UrgentReconciliationPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const scope: ReturnType<typeof useOperationsScope> = useOperationsScope();
  const queryClient: QueryClient = useQueryClient();
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canRead: boolean = permissions.includes("business.reconciliation.read");
  const canManage: boolean = permissions.includes("business.urgent_work.reconcile");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [evidence, setEvidence] = useState<EvidenceDraft>({
    customerId: "",
    startedAt: "",
    endedAt: "",
    reference: "",
    notes: "",
  });
  const [finalDraft, setFinalDraft] = useState<FinalDraft>({
    customerId: "",
    jobId: "",
    hours: "",
    reason: "",
    useManualRate: false,
    currency: "VND",
    billRate: "",
    workerRate: "",
    manualRateReason: "",
  });
  const [message, setMessage] = useState<string | null>(null);

  const reconciliationQuery = useInfiniteQuery({
    queryKey: [
      ...operationsQueryKeys.urgentReconciliations,
      "scope",
      scope.scopeKey,
      "customer",
      scope.selectedCustomerId ?? "all",
    ],
    initialPageParam: createReconciliationScopeCursor<UrgentWorkReconciliation>(scope.branchIds),
    queryFn: ({ pageParam }: { pageParam: ReconciliationScopeCursor<UrgentWorkReconciliation> }): Promise<ReconciliationScopePage<UrgentWorkReconciliation>> =>
      loadReconciliationScopePage<UrgentWorkReconciliation>({
        cursor: pageParam,
        fetchBranchPage: (
          branchId: string,
          cursor: string | null,
        ): Promise<UrgentReconciliationPageResponse> =>
          listUrgentReconciliationsForBranch(
            branchId,
            cursor,
            scope.selectedCustomerId,
          ),
        compare: compareUrgentReconciliations,
        itemKey: (item: UrgentWorkReconciliation): string => item.work.report_id,
      }),
    getNextPageParam: (
      lastPage: ReconciliationScopePage<UrgentWorkReconciliation>,
    ): ReconciliationScopeCursor<UrgentWorkReconciliation> | undefined => lastPage.nextCursor ?? undefined,
    enabled: canRead && scope.branchIds.length > 0,
  });
  const loadedPages: ReconciliationScopePage<UrgentWorkReconciliation>[] =
    reconciliationQuery.data?.pages ?? [];
  const pageItems: UrgentWorkReconciliation[] = loadedPages[currentPage - 1]?.items ?? [];
  const hasNextPage: boolean = currentPage < loadedPages.length || reconciliationQuery.hasNextPage;
  const selected: UrgentWorkReconciliation | null =
    pageItems.find((item: UrgentWorkReconciliation): boolean => item.work.report_id === selectedId) ?? null;

  useEffect((): void => {
    setCurrentPage(1);
  }, [scope.scopeKey, scope.selectedCustomerId]);

  useEffect((): void => {
    if (loadedPages.length > 0 && currentPage > loadedPages.length) {
      setCurrentPage(loadedPages.length);
    }
  }, [currentPage, loadedPages.length]);

  const changePage = (nextPage: number): void => {
    if (nextPage < 1) {
      return;
    }
    if (nextPage <= loadedPages.length) {
      setCurrentPage(nextPage);
      return;
    }
    if (nextPage === loadedPages.length + 1 && reconciliationQuery.hasNextPage) {
      void reconciliationQuery.fetchNextPage().then((result): void => {
        if ((result.data?.pages.length ?? 0) >= nextPage) {
          setCurrentPage(nextPage);
        }
      });
    }
  };

  useEffect((): void => {
    const selectionRemainsVisible: boolean = pageItems.some(
      (item: UrgentWorkReconciliation): boolean => item.work.report_id === selectedId,
    );
    if (!selectionRemainsVisible) {
      setSelectedId(pageItems.at(0)?.work.report_id ?? null);
    }
  }, [pageItems, selectedId]);

  const selectedBranchId: string | null = selected?.work.branch_id ?? null;
  const customersQuery: UseQueryResult<UrgentWorkCustomer[], Error> = useQuery({
    queryKey: [...operationsQueryKeys.urgentCustomers, "branch", selectedBranchId],
    queryFn: (): Promise<UrgentWorkCustomer[]> =>
      listUrgentCustomersForBranch(selectedBranchId ?? ""),
    enabled: canRead && selectedBranchId !== null,
  });
  const jobsQuery: UseQueryResult<StaffingJob[], Error> = useQuery({
    queryKey: [...operationsQueryKeys.jobs, "branch", selectedBranchId],
    queryFn: (): Promise<StaffingJob[]> => listJobsForBranch(selectedBranchId ?? ""),
    enabled: canRead && selectedBranchId !== null,
  });

  useEffect((): void => {
    if (!selected) {
      return;
    }
    setEvidence(initialEvidence(selected));
    setFinalDraft(initialFinal(selected));
    setMessage(null);
  }, [selected]);

  useEffect((): void => {
    const jobs: StaffingJob[] = jobsQuery.data ?? [];
    if (!selectedId || jobs.length !== 1) {
      return;
    }
    setFinalDraft((current: FinalDraft): FinalDraft =>
      current.jobId ? current : { ...current, jobId: jobs[0].id },
    );
  }, [jobsQuery.data, selectedId]);

  const refresh = (): Promise<void> =>
    queryClient.invalidateQueries({ queryKey: operationsQueryKeys.urgentReconciliations });

  const evidenceMutation: UseMutationResult<UrgentCustomerWorkRecord, unknown, void> = useMutation<
    UrgentCustomerWorkRecord,
    unknown,
    void
  >({
    mutationFn: (): Promise<UrgentCustomerWorkRecord> => {
      if (!selectedId || !selected) {
        return Promise.reject(new Error("urgent report is not selected"));
      }
      return saveUrgentCustomerWorkRecordForBranch(selected.work.branch_id, selectedId, {
        confirmed_customer_id: evidence.customerId,
        confirmed_started_at: new Date(evidence.startedAt).toISOString(),
        confirmed_ended_at: new Date(evidence.endedAt).toISOString(),
        customer_reference: evidence.reference.trim() || null,
        notes: evidence.notes.trim() || null,
      });
    },
    onSuccess: (): void => {
      setMessage("Đã lưu thông tin khách hàng xác nhận.");
      void refresh();
    },
    onError: (error: unknown): void => {
      setMessage(friendlyApiError(error, "Không thể lưu thông tin khách hàng xác nhận."));
    },
  });

  const reconcileMutation: UseMutationResult<UrgentWorkReconciliation, unknown, void> = useMutation<
    UrgentWorkReconciliation,
    unknown,
    void
  >({
    mutationFn: (): Promise<UrgentWorkReconciliation> => {
      if (!selectedId || !selected) {
        return Promise.reject(new Error("urgent report is not selected"));
      }
      const manualRate: ManualRateOverrideRequest | null = finalDraft.useManualRate
        ? {
            reason: finalDraft.manualRateReason.trim(),
            currency: finalDraft.currency.trim().toUpperCase(),
            bill_hourly_rate: finalDraft.billRate.trim(),
            worker_hourly_rate: finalDraft.workerRate.trim(),
          }
        : null;
      return reconcileUrgentWorkForBranch(selected.work.branch_id, selectedId, {
        final_customer_id: finalDraft.customerId,
        job_id: finalDraft.jobId,
        worked_seconds: Math.round(Number(finalDraft.hours) * 3600),
        adjustment_reason: finalDraft.reason.trim() || null,
        manual_rate: manualRate,
      });
    },
    onSuccess: (): void => {
      setMessage("Đã chốt kết quả cuối cùng cho tính lương và doanh thu.");
      void refresh();
    },
    onError: (error: unknown): void => {
      setMessage(
        friendlyApiError(error, "Không thể chốt đối soát. Mọi chênh lệch về khách hàng hoặc thời gian cần có lý do."),
      );
    },
  });

  const acceptStaffRecordMutation: UseMutationResult<UrgentWorkReconciliation, unknown, void> = useMutation<
    UrgentWorkReconciliation,
    unknown,
    void
  >({
    mutationFn: (): Promise<UrgentWorkReconciliation> => {
      if (!selectedId || !selected || !finalDraft.jobId) {
        throw new Error("urgent staff work record is incomplete");
      }
      return acceptUrgentStaffRecordForBranch(selected.work.branch_id, selectedId, {
        job_id: finalDraft.jobId,
      });
    },
    onSuccess: (): void => {
      setMessage("Đã xác nhận giờ nhân viên và chốt kết quả công việc.");
      void refresh();
    },
    onError: (error: unknown): void => {
      setMessage(
        friendlyApiError(error, "Không thể xác nhận. Dữ liệu khách hàng phải khớp hoàn toàn với giờ nhân viên."),
      );
    },
  });

  const saveEvidence = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    setMessage(null);
    evidenceMutation.mutate();
  };

  if (!canRead) {
    return <section className="panel p-8 text-center text-sm text-slate-500">Bạn chưa có quyền xem đối soát.</section>;
  }

  const isPending: boolean =
    reconciliationQuery.isPending ||
    (selectedBranchId !== null && (customersQuery.isPending || jobsQuery.isPending));
  const firstError: unknown = reconciliationQuery.error ?? customersQuery.error ?? jobsQuery.error;
  if (isPending) {
    return (
      <section className="panel p-8 text-center text-sm text-slate-500">
        <RefreshCw className="mr-2 inline size-4 animate-spin" />
        Đang tải dữ liệu đối soát...
      </section>
    );
  }
  if (firstError) {
    return (
      <section className="panel p-8 text-center text-sm text-red-600">
        <CircleAlert className="mr-2 inline size-4" />
        {friendlyApiError(firstError, "Không thể tải dữ liệu đối soát.")}
      </section>
    );
  }

  return (
    <div className="space-y-4">
      <ReconciliationModeSelector mode="urgent" />

      <label className="panel block p-4 text-sm font-semibold text-slate-700 md:hidden">
        Chọn công việc cần đối soát
        <select
          className="mt-2 min-h-11 w-full rounded-xl border-slate-300 bg-white px-3"
          onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setSelectedId(event.target.value || null)}
          value={selectedId ?? ""}
        >
          <option value="">Chọn công việc</option>
          {pageItems.map((item: UrgentWorkReconciliation): React.JSX.Element => (
            <option key={item.work.report_id} value={item.work.report_id}>
              {item.work.employee_name} · {item.work.customer_name} · {statusLabel(item.reconciliation_status)}
            </option>
          ))}
        </select>
        {pageItems.length === 0 ? <p className="mt-2 font-normal text-slate-500">Chưa có công việc phát sinh cần đối soát.</p> : null}
      </label>
      <ReconciliationPagination className="panel rounded-2xl md:hidden" currentItemCount={pageItems.length} currentPage={currentPage} hasNextPage={hasNextPage} nextPagePending={reconciliationQuery.isFetchingNextPage} onPageChange={changePage} />

      <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(280px,0.72fr)_minmax(0,1.28fr)]">
        <section className="panel hidden overflow-hidden md:block">
          <div className="border-b border-slate-200 px-5 py-4">
            <h2 className="font-bold text-slate-950">Công việc cần đối soát</h2>
            <p className="mt-1 text-sm text-slate-500">Kiểm tra nơi làm việc và thời gian nhân viên đã ghi nhận.</p>
          </div>
          <div className="max-h-[72vh] divide-y divide-slate-100 overflow-y-auto">
            {pageItems.map((item: UrgentWorkReconciliation): React.JSX.Element => (
              <button
                className={`w-full px-5 py-4 text-left hover:bg-slate-50 ${selectedId === item.work.report_id ? "bg-blue-50" : ""}`}
                aria-pressed={selectedId === item.work.report_id}
                key={item.work.report_id}
                onClick={(): void => setSelectedId(item.work.report_id)}
                type="button"
              >
                <div className="flex items-start justify-between gap-2">
                  <div>
                    <p className="font-bold text-slate-900">{item.work.employee_name}</p>
                    <p className="mt-1 text-xs text-slate-500">
                      {item.work.customer_name} · {item.work.branch_name}
                    </p>
                  </div>
                  <span className={`shrink-0 rounded-full px-2.5 py-1 text-[11px] font-bold ${statusTone(item.reconciliation_status)}`}>
                    {statusLabel(item.reconciliation_status)}
                  </span>
                </div>
                <p className="mt-2 text-xs text-slate-500">{formatDateTime(item.work.started_at)}</p>
              </button>
            ))}
            {pageItems.length === 0 ? (
              <p className="p-8 text-center text-sm text-slate-500">Chưa có công việc phát sinh cần đối soát.</p>
            ) : null}
          </div>
          <ReconciliationPagination currentItemCount={pageItems.length} currentPage={currentPage} hasNextPage={hasNextPage} nextPagePending={reconciliationQuery.isFetchingNextPage} onPageChange={changePage} />
        </section>

        {selected ? (
          <div className="min-w-0 space-y-5">
            {message ? (
              <div className="rounded-xl border border-blue-200 bg-blue-50 px-4 py-3 text-sm font-medium text-blue-800">
                {message}
              </div>
            ) : null}

            <section className="panel p-4 sm:p-6">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h2 className="text-lg font-black text-slate-950">{selected.work.employee_name}</h2>
                  <p className="mt-1 text-sm text-slate-500">
                    {selected.work.employee_code} · {selected.work.customer_name} · {selected.work.branch_name}
                  </p>
                </div>
                <span className={`rounded-full px-3 py-1 text-xs font-bold ${statusTone(selected.reconciliation_status)}`}>
                  {statusLabel(selected.reconciliation_status)}
                </span>
              </div>
              <div className="mt-5 grid gap-3 md:grid-cols-3">
                <div className="rounded-xl bg-violet-50 p-4">
                  <p className="text-xs font-bold uppercase text-violet-600">Nhân viên khai</p>
                  <p className="mt-2 font-black text-violet-950">{selected.work.customer_name}</p>
                  <p className="mt-1 text-sm font-bold text-violet-900">
                    {formatDuration(selected.work.worked_seconds ?? 0)}
                  </p>
                  <p className="mt-1 text-xs text-violet-700">
                    {formatDateTime(selected.work.started_at)} → {selected.work.ended_at ? formatDateTime(selected.work.ended_at) : "đang làm"}
                  </p>
                </div>
                <div className="rounded-xl bg-amber-50 p-4">
                  <p className="text-xs font-bold uppercase text-amber-600">Khách hàng xác nhận</p>
                  <p className="mt-2 font-black text-amber-950">
                    {selected.customer_record?.confirmed_customer_name ?? "Chưa nhập"}
                  </p>
                  <p className="mt-1 text-sm font-bold text-amber-900">
                    {selected.customer_record ? formatDuration(selected.customer_record.confirmed_worked_seconds) : "—"}
                  </p>
                  <p className="mt-1 text-xs text-amber-700">Theo xác nhận khách hàng</p>
                </div>
                <div className="rounded-xl bg-emerald-50 p-4">
                  <p className="text-xs font-bold uppercase text-emerald-600">Kết quả cuối</p>
                  <p className="mt-2 font-black text-emerald-950">
                    {selected.final_worked_seconds ? formatDuration(selected.final_worked_seconds) : "Chưa chốt"}
                  </p>
                  <p className="mt-1 text-xs text-emerald-700">Nguồn cho bill và lương</p>
                </div>
              </div>
            </section>

            <form className="panel p-4 sm:p-6" onSubmit={saveEvidence}>
              <div className="flex items-center gap-2">
                <MapPin className="size-5 text-amber-600" />
                <h3 className="font-bold text-slate-950">Xác nhận / bill từ khách hàng</h3>
              </div>
              <p className="mt-1 text-sm text-slate-500">
                Nhập khách hàng và thời gian theo bill hoặc thông tin khách hàng đã xác nhận.
              </p>
              <label className="mt-4 block text-sm font-semibold text-slate-700">
                Khách hàng / nơi làm việc xác nhận
                <select
                  className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
                  disabled={!canManage || selected.reconciliation_status === "reconciled"}
                  onChange={(event: React.ChangeEvent<HTMLSelectElement>): void =>
                    setEvidence((current: EvidenceDraft): EvidenceDraft => ({ ...current, customerId: event.target.value }))
                  }
                  required
                  value={evidence.customerId}
                >
                  <option value="">Chọn khách hàng theo bill</option>
                  {(customersQuery.data ?? []).map((customer: UrgentWorkCustomer): React.JSX.Element => (
                    <option key={customer.customer_id} value={customer.customer_id}>
                      {customer.customer_name}
                    </option>
                  ))}
                </select>
              </label>
              <div className="mt-3 grid min-w-0 gap-3 sm:grid-cols-2">
                <label className="min-w-0 text-sm font-semibold text-slate-700">
                  Bắt đầu xác nhận
                  <input
                    className="mt-1.5 min-w-0 w-full max-w-full rounded-xl border border-slate-200 px-3 py-2.5"
                    disabled={!canManage || selected.reconciliation_status === "reconciled"}
                    onChange={(event: React.ChangeEvent<HTMLInputElement>): void =>
                      setEvidence((current: EvidenceDraft): EvidenceDraft => ({ ...current, startedAt: event.target.value }))
                    }
                    required
                    type="datetime-local"
                    value={evidence.startedAt}
                  />
                </label>
                <label className="min-w-0 text-sm font-semibold text-slate-700">
                  Kết thúc xác nhận
                  <input
                    className="mt-1.5 min-w-0 w-full max-w-full rounded-xl border border-slate-200 px-3 py-2.5"
                    disabled={!canManage || selected.reconciliation_status === "reconciled"}
                    onChange={(event: React.ChangeEvent<HTMLInputElement>): void =>
                      setEvidence((current: EvidenceDraft): EvidenceDraft => ({ ...current, endedAt: event.target.value }))
                    }
                    required
                    type="datetime-local"
                    value={evidence.endedAt}
                  />
                </label>
              </div>
              <label className="mt-3 block text-sm font-semibold text-slate-700">
                Mã bill / tham chiếu
                <input
                  className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5"
                  disabled={!canManage || selected.reconciliation_status === "reconciled"}
                  onChange={(event: React.ChangeEvent<HTMLInputElement>): void =>
                    setEvidence((current: EvidenceDraft): EvidenceDraft => ({ ...current, reference: event.target.value }))
                  }
                  value={evidence.reference}
                />
              </label>
              <label className="mt-3 block text-sm font-semibold text-slate-700">
                Ghi chú
                <textarea
                  className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5"
                  disabled={!canManage || selected.reconciliation_status === "reconciled"}
                  onChange={(event: React.ChangeEvent<HTMLTextAreaElement>): void =>
                    setEvidence((current: EvidenceDraft): EvidenceDraft => ({ ...current, notes: event.target.value }))
                  }
                  rows={2}
                  value={evidence.notes}
                />
              </label>
              {selected.reconciliation_status !== "reconciled" ? (
                <button className="action-secondary mt-4 w-full sm:w-auto" disabled={!canManage || evidenceMutation.isPending} type="submit">
                  <Save className="size-4" />
                  {evidenceMutation.isPending ? "Đang lưu..." : "Lưu bằng chứng khách hàng"}
                </button>
              ) : null}
            </form>

            <section className="panel p-4 sm:p-6">
              <div className="flex items-center gap-2">
                <GitCompareArrows className="size-5 text-blue-600" />
                <h3 className="font-bold text-slate-950">Chốt đối soát</h3>
              </div>
              {selected.reconciliation_status === "matched" ? (
                <div className="mt-4 rounded-2xl border border-emerald-200 bg-emerald-50 p-4">
                  <p className="font-bold text-emerald-950">Hai nguồn đã khớp</p>
                  <p className="mt-1 text-sm leading-6 text-emerald-800">
                    Xác nhận kết quả khi nơi làm việc, giờ nhân viên và dữ liệu khách hàng đã khớp hoàn toàn. Thao tác này không thay đổi dữ liệu của hai bên.
                  </p>
                  {!finalDraft.jobId ? (
                    <p className="mt-2 text-sm font-semibold text-amber-700">Vui lòng chọn công việc bên dưới trước khi xác nhận.</p>
                  ) : null}
                  <button
                    className="mt-3 inline-flex min-h-11 w-full items-center justify-center gap-2 rounded-xl bg-emerald-700 px-4 text-sm font-bold text-white hover:bg-emerald-800 disabled:cursor-not-allowed disabled:opacity-50 sm:w-auto"
                    disabled={
                      !canManage ||
                      !finalDraft.jobId ||
                      acceptStaffRecordMutation.isPending ||
                      evidenceMutation.isPending ||
                      reconcileMutation.isPending
                    }
                    onClick={(): void => {
                      if (window.confirm("Xác nhận hai nguồn đã khớp và khóa kết quả công việc này?")) {
                        acceptStaffRecordMutation.mutate();
                      }
                    }}
                    type="button"
                  >
                    <CheckCircle2 className="size-4" />
                    {acceptStaffRecordMutation.isPending ? "Đang xác nhận..." : "Xác nhận giờ nhân viên"}
                  </button>
                </div>
              ) : null}
              {selected.reconciliation_status !== "reconciled" && selected.reconciliation_status !== "matched" ? (
                <p className="mt-4 rounded-xl bg-amber-50 px-4 py-3 text-sm text-amber-800">
                  Nhập dữ liệu khách hàng trước. Nút xác nhận nhanh chỉ xuất hiện khi khách hàng và nhân viên ghi nhận hoàn toàn giống nhau.
                </p>
              ) : null}
              <div className="mt-4 grid min-w-0 gap-3 sm:grid-cols-2">
                <label className="text-sm font-semibold text-slate-700">
                  Khách hàng cuối cùng
                  <select
                    className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
                    disabled={selected.reconciliation_status === "reconciled"}
                    onChange={(event: React.ChangeEvent<HTMLSelectElement>): void =>
                      setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, customerId: event.target.value }))
                    }
                    value={finalDraft.customerId}
                  >
                    <option value="">Chọn khách hàng cuối</option>
                    {(customersQuery.data ?? []).map((customer: UrgentWorkCustomer): React.JSX.Element => (
                      <option key={customer.customer_id} value={customer.customer_id}>
                        {customer.customer_name}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="text-sm font-semibold text-slate-700">
                  Công việc / vị trí
                  <select
                    className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
                    disabled={selected.reconciliation_status === "reconciled"}
                    onChange={(event: React.ChangeEvent<HTMLSelectElement>): void =>
                      setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, jobId: event.target.value }))
                    }
                    value={finalDraft.jobId}
                  >
                    <option value="">Chọn công việc</option>
                    {(jobsQuery.data ?? []).map((job: StaffingJob): React.JSX.Element => (
                      <option key={job.id} value={job.id}>{job.name}</option>
                    ))}
                  </select>
                </label>
                <label className="text-sm font-semibold text-slate-700">
                  Thời gian cuối (giờ)
                  <input
                    className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5"
                    disabled={selected.reconciliation_status === "reconciled"}
                    min="0.01"
                    onChange={(event: React.ChangeEvent<HTMLInputElement>): void =>
                      setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, hours: event.target.value }))
                    }
                    step="0.01"
                    type="number"
                    value={finalDraft.hours}
                  />
                </label>
                <label className="text-sm font-semibold text-slate-700">
                  Lý do xử lý chênh lệch
                  <input
                    className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5"
                    disabled={selected.reconciliation_status === "reconciled"}
                    onChange={(event: React.ChangeEvent<HTMLInputElement>): void =>
                      setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, reason: event.target.value }))
                    }
                    placeholder="Bắt buộc nếu khách hàng hoặc thời gian lệch"
                    value={finalDraft.reason}
                  />
                </label>
              </div>

              <label className="mt-4 flex items-start gap-3 text-sm font-semibold leading-5 text-slate-700">
                <input
                  checked={finalDraft.useManualRate}
                  disabled={selected.reconciliation_status === "reconciled"}
                  onChange={(event: React.ChangeEvent<HTMLInputElement>): void =>
                    setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, useManualRate: event.target.checked }))
                  }
                  type="checkbox"
                />
                Nhập đơn giá thủ công nếu chưa có thỏa thuận giá phù hợp
              </label>
              {finalDraft.useManualRate ? (
                <div className="mt-3 grid gap-3 sm:grid-cols-3">
                  <label className="text-sm font-semibold text-slate-700 sm:col-span-3">Lý do dùng giá thủ công<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, manualRateReason: event.target.value }))} placeholder="Ví dụ: khách hàng xác nhận đơn giá ngoài bảng giá hiện hành" value={finalDraft.manualRateReason} /></label>
                  <label className="text-sm font-semibold text-slate-700">Tiền tệ<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5 uppercase" maxLength={3} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, currency: event.target.value }))} value={finalDraft.currency} /></label>
                  <label className="text-sm font-semibold text-slate-700">Đơn giá khách hàng<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" inputMode="decimal" onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, billRate: event.target.value }))} value={finalDraft.billRate} /></label>
                  <label className="text-sm font-semibold text-slate-700">Đơn giá trả nhân viên<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" inputMode="decimal" onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, workerRate: event.target.value }))} value={finalDraft.workerRate} /></label>
                </div>
              ) : null}

              {selected.reconciliation_status === "reconciled" ? (
                <p className="mt-4 flex items-center gap-2 text-sm font-semibold text-emerald-700">
                  <CheckCircle2 className="size-5" />
                  Kết quả đã khóa và sẵn sàng cho bill, lương và biên lợi nhuận.
                </p>
              ) : (
                <button
                  className="action-primary mt-4 w-full sm:w-auto"
                  disabled={
                    !canManage ||
                    !selected.customer_record ||
                    !finalDraft.customerId ||
                    !finalDraft.jobId ||
                    Number(finalDraft.hours) <= 0 ||
                    reconcileMutation.isPending
                  }
                  onClick={(): void => reconcileMutation.mutate()}
                  type="button"
                >
                  <CheckCircle2 className="size-4" />
                  {reconcileMutation.isPending ? "Đang chốt..." : "Chốt kết quả"}
                </button>
              )}
            </section>
          </div>
        ) : null}
      </div>
    </div>
  );
}
