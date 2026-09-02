import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, CircleAlert, GitCompareArrows, RefreshCw, Save } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import type {
  Customer,
  ReconcileStatus,
  StaffingJob,
  StaffingReconcile,
  StaffingReconcilePageRsp,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { formatDateTime, formatDuration } from "../../shared/lib/format";
import { useAuth } from "../auth/AuthProvider";
import {
  acceptAssignmentStaffRecordForBranch,
  correctReconciliationForBranch,
  listCustomersForBranch,
  listJobsForBranch,
  listReconciliationsForBranch,
  operationsQueryKeys,
  reconcileAssignmentForBranch,
  saveCustomerWorkRecordForBranch,
} from "./api";
import { useOperationsScope } from "./OperationsScopeProvider";
import { ReconciliationModeSelector } from "./ReconciliationModeSelector";
import { ReconciliationPagination } from "./ReconciliationPagination";
import {
  createReconciliationScopeCursor,
  loadReconcileScopePage,
  type ReconciliationScopeCursor,
  type ReconcileScopePage,
} from "./reconciliationCursor";

interface ScopedStaffingReconcile extends StaffingReconcile {
  branch_id: string;
}

function compareScopedReconciliations(
  left: ScopedStaffingReconcile,
  right: ScopedStaffingReconcile,
): number {
  const timeDifference: number =
    new Date(right.scheduled_starts_at).getTime() - new Date(left.scheduled_starts_at).getTime();
  if (timeDifference !== 0) {
    return timeDifference;
  }
  if (left.assignment_id === right.assignment_id) {
    return 0;
  }
  return left.assignment_id < right.assignment_id ? 1 : -1;
}

function localDateTime(value: string | null | undefined): string {
  if (!value) return "";
  const date = new Date(value);
  const offset = date.getTimezoneOffset() * 60_000;
  return new Date(date.getTime() - offset).toISOString().slice(0, 19);
}

function statusLabel(status: ReconcileStatus): string {
  const labels: Record<ReconcileStatus, string> = {
    pending_staff: "Chờ nhân viên kết thúc",
    pending_customer: "Chờ dữ liệu khách hàng",
    matched: "Khớp",
    discrepancy: "Chênh lệch",
    reconciled: "Đã chốt",
  };
  return labels[status];
}

function statusTone(status: ReconcileStatus): string {
  if (status === "matched" || status === "reconciled") return "bg-emerald-50 text-emerald-700";
  if (status === "discrepancy") return "bg-red-50 text-red-700";
  return "bg-amber-50 text-amber-700";
}

interface EvidenceDraft {
  customerId: string;
  startedAt: string;
  startedAtExact: string | null;
  endedAt: string;
  endedAtExact: string | null;
  reference: string;
  notes: string;
}

export function ReconciliationPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const scope: ReturnType<typeof useOperationsScope> = useOperationsScope();
  const queryClient: ReturnType<typeof useQueryClient> = useQueryClient();
  const canRead: boolean = auth.profile?.permissions.includes("business.reconciliation.read") ?? false;
  const canManage: boolean = auth.profile?.permissions.includes("business.reconciliation.manage") ?? false;
  const canCorrect: boolean = auth.profile?.permissions.includes("business.reconciliation.correct") ?? false;
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [collection, setCollection] = useState<"pending" | "confirmed">("pending");
  const today = new Date();
  const [periodStart, setPeriodStart] = useState<string>(() => new Date(today.getFullYear(), today.getMonth(), 1).toISOString().slice(0, 10));
  const [periodEnd, setPeriodEnd] = useState<string>(() => new Date(today.getFullYear(), today.getMonth() + 1, 0).toISOString().slice(0, 10));
  const [evidence, setEvidence] = useState<EvidenceDraft>({
    customerId: "",
    startedAt: "",
    startedAtExact: null,
    endedAt: "",
    endedAtExact: null,
    reference: "",
    notes: "",
  });
  const [finalHours, setFinalHours] = useState("");
  const [finalCustomerId, setFinalCustomerId] = useState("");
  const [finalJobId, setFinalJobId] = useState("");
  const [resolution, setResolution] = useState("");
  const [correctionReason, setCorrectionReason] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  const query = useInfiniteQuery({
    queryKey: [
      ...operationsQueryKeys.reconciliations,
      "scope",
      scope.scopeKey,
      "customer",
      scope.selectedCustomerId ?? "all",
      collection,
      periodStart,
      periodEnd,
    ],
    initialPageParam: createReconciliationScopeCursor<ScopedStaffingReconcile>(scope.branchIds),
    queryFn: ({ pageParam }: { pageParam: ReconciliationScopeCursor<ScopedStaffingReconcile> }): Promise<ReconcileScopePage<ScopedStaffingReconcile>> =>
      loadReconcileScopePage<ScopedStaffingReconcile>({
        cursor: pageParam,
        fetchBranchPage: async (
          branchId: string,
          cursor: string | null,
        ): Promise<StaffingReconcilePageRsp & { items: ScopedStaffingReconcile[] }> => {
          const page: StaffingReconcilePageRsp = await listReconciliationsForBranch(
            branchId,
            cursor,
            scope.selectedCustomerId,
            collection,
            collection === "confirmed" ? new Date(`${periodStart}T00:00:00`).toISOString() : null,
            collection === "confirmed" ? new Date(`${periodEnd}T23:59:59.999`).toISOString() : null,
          );
          return {
            ...page,
            items: page.items.map(
              (item: StaffingReconcile): ScopedStaffingReconcile => ({ ...item, branch_id: branchId }),
            ),
          };
        },
        compare: compareScopedReconciliations,
        itemKey: (item: ScopedStaffingReconcile): string => item.assignment_id,
      }),
    getNextPageParam: (
      lastPage: ReconcileScopePage<ScopedStaffingReconcile>,
    ): ReconciliationScopeCursor<ScopedStaffingReconcile> | undefined => lastPage.nextCursor ?? undefined,
    enabled: canRead && scope.branchIds.length > 0,
  });
  const loadedPages: ReconcileScopePage<ScopedStaffingReconcile>[] = query.data?.pages ?? [];
  const pageItems: ScopedStaffingReconcile[] = loadedPages[currentPage - 1]?.items ?? [];
  const hasNextPage: boolean = currentPage < loadedPages.length || query.hasNextPage;
  const selected: ScopedStaffingReconcile | null =
    pageItems.find(
      (item: ScopedStaffingReconcile): boolean => item.assignment_id === selectedId,
    ) ?? null;

  const customersQuery = useInfiniteQuery({
    queryKey: [...operationsQueryKeys.customers, "branch", selected?.branch_id ?? "none"],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => listCustomersForBranch(selected?.branch_id ?? "", pageParam),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    enabled: canRead && selected !== null,
  });
  const customers: Customer[] = customersQuery.data?.pages.flatMap((page) => page.items) ?? [];
  const jobsQuery = useInfiniteQuery({
    queryKey: [...operationsQueryKeys.jobs, "branch", selected?.branch_id ?? "none"],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => listJobsForBranch(selected?.branch_id ?? "", pageParam),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    enabled: canRead && selected !== null,
  });
  const jobs: StaffingJob[] = jobsQuery.data?.pages.flatMap((page) => page.items) ?? [];

  useEffect((): void => {
    const selectionRemainsVisible: boolean = pageItems.some(
      (item: ScopedStaffingReconcile): boolean => item.assignment_id === selectedId,
    );
    if (!selectionRemainsVisible) {
      setSelectedId(pageItems.at(0)?.assignment_id ?? null);
    }
  }, [pageItems, selectedId]);

  useEffect((): void => {
    setCurrentPage(1);
  }, [scope.scopeKey, scope.selectedCustomerId, collection, periodStart, periodEnd]);

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
    if (nextPage === loadedPages.length + 1 && query.hasNextPage) {
      void query.fetchNextPage().then((result): void => {
        if ((result.data?.pages.length ?? 0) >= nextPage) {
          setCurrentPage(nextPage);
        }
      });
    }
  };

  useEffect(() => {
    if (!selected) return;
    const exactStartedAt: string | null = selected.customer_record?.confirmed_started_at ?? selected.staff_started_at;
    const exactEndedAt: string | null = selected.customer_record?.confirmed_ended_at ?? selected.staff_ended_at;
    setEvidence({
      customerId: selected.customer_record?.confirmed_customer_id ?? selected.customer_id,
      startedAt: localDateTime(exactStartedAt),
      startedAtExact: exactStartedAt,
      endedAt: localDateTime(exactEndedAt),
      endedAtExact: exactEndedAt,
      reference: selected.customer_record?.customer_reference ?? "00000000",
      notes: selected.customer_record?.notes ?? "",
    });
    const seconds = selected.final_worked_seconds ?? selected.customer_record?.confirmed_worked_seconds ?? selected.staff_worked_seconds;
    setFinalHours(seconds > 0 ? (seconds / 3600).toFixed(4) : "");
    setFinalCustomerId(selected.final_customer_id ?? selected.customer_id);
    setFinalJobId(selected.final_job_id ?? selected.job_id);
    setResolution(selected.adjustment_reason ?? "");
  }, [selected]);

  const refresh = () => queryClient.invalidateQueries({ queryKey: operationsQueryKeys.reconciliations });
  const evidenceMutation = useMutation({
    mutationFn: () => saveCustomerWorkRecordForBranch(selected?.branch_id ?? "", selectedId ?? "", {
      confirmed_customer_id: evidence.customerId,
      confirmed_started_at: evidence.startedAtExact ?? new Date(evidence.startedAt).toISOString(),
      confirmed_ended_at: evidence.endedAtExact ?? new Date(evidence.endedAt).toISOString(),
      customer_reference: evidence.reference.trim() || null,
      notes: evidence.notes.trim() || null,
    }),
    onSuccess: () => { setMessage("Đã lưu dữ liệu xác nhận từ khách hàng."); void refresh(); },
    onError: (error) => setMessage(friendlyApiError(error, "Không thể lưu dữ liệu khách hàng.")),
  });
  const reconcileMutation = useMutation({
    mutationFn: () => reconcileAssignmentForBranch(selected?.branch_id ?? "", selectedId ?? "", {
      worked_seconds: Math.round(Number(finalHours) * 3600),
      adjustment_reason: resolution.trim() || null,
      final_customer_id: finalCustomerId !== selected?.customer_id ? finalCustomerId : null,
      final_job_id: finalJobId !== selected?.job_id ? finalJobId : null,
    }),
    onSuccess: () => { setMessage("Đã chốt dữ liệu cuối cùng và khóa kết quả ca."); void refresh(); },
    onError: (error) => setMessage(friendlyApiError(error, "Không thể chốt đối soát. Chênh lệch cần có lý do xử lý.")),
  });
  const acceptStaffRecordMutation = useMutation({
    mutationFn: () => acceptAssignmentStaffRecordForBranch(selected?.branch_id ?? "", selectedId ?? ""),
    onSuccess: () => {
      setMessage("Đã xác nhận giờ nhân viên và chốt kết quả ca làm.");
      void refresh();
    },
    onError: (error) => setMessage(friendlyApiError(error, "Không thể xác nhận. Dữ liệu khách hàng phải khớp hoàn toàn với giờ nhân viên.")),
  });
  const correctionMutation = useMutation({
    mutationFn: () => correctReconciliationForBranch(selected?.branch_id ?? "", selectedId ?? "", {
      expected_revision_id: selected?.result_revision_id ?? "",
      worked_seconds: Math.round(Number(finalHours) * 3600),
      correction_reason: correctionReason.trim(),
    }),
    onSuccess: () => { setMessage("Đã lưu phiên bản điều chỉnh mới; kết quả cũ vẫn được giữ lại."); setCorrectionReason(""); void refresh(); },
    onError: (error) => setMessage(friendlyApiError(error, "Không thể điều chỉnh. Hãy tải lại nếu kết quả vừa được người khác thay đổi hoặc kỳ tài chính đã khóa.")),
  });

  const saveEvidence = (event: FormEvent) => {
    event.preventDefault();
    setMessage(null);
    evidenceMutation.mutate();
  };

  const copyStaffEvidence = (): void => {
    if (!selected?.staff_started_at || !selected.staff_ended_at) return;
    setEvidence((current: EvidenceDraft): EvidenceDraft => ({
      ...current,
      customerId: selected.customer_id,
      startedAt: localDateTime(selected.staff_started_at),
      startedAtExact: selected.staff_started_at,
      endedAt: localDateTime(selected.staff_ended_at),
      endedAtExact: selected.staff_ended_at,
      reference: current.reference.trim() || "00000000",
    }));
  };

  if (!canRead) return <section className="panel p-8 text-center text-sm text-slate-500">Bạn chưa có quyền xem đối soát.</section>;
  if (query.isPending) return <section className="panel p-8 text-center text-sm text-slate-500"><RefreshCw className="mr-2 inline size-4 animate-spin" />Đang tải dữ liệu đối soát...</section>;
  if (query.error) return <section className="panel p-8 text-center text-sm text-red-600"><CircleAlert className="mr-2 inline size-4" />{friendlyApiError(query.error, "Không thể tải đối soát.")}</section>;

  return (
    <div className="space-y-4">
      <ReconciliationModeSelector mode="planned" />

      <section className="panel p-3 sm:p-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <button className={`min-h-11 rounded-xl px-4 text-sm font-bold ${collection === "pending" ? "bg-blue-700 text-white" : "bg-slate-100 text-slate-700"}`} onClick={() => setCollection("pending")} type="button">Cần đối soát</button>
          <button className={`min-h-11 rounded-xl px-4 text-sm font-bold ${collection === "confirmed" ? "bg-emerald-700 text-white" : "bg-slate-100 text-slate-700"}`} onClick={() => setCollection("confirmed")} type="button">Đã xác nhận / đối soát</button>
        </div>
        {collection === "confirmed" ? <div className="mt-3 grid gap-3 sm:grid-cols-2"><label className="text-sm font-semibold text-slate-700">Từ ngày<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" type="date" value={periodStart} onChange={(event) => setPeriodStart(event.target.value)} /></label><label className="text-sm font-semibold text-slate-700">Đến ngày<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" min={periodStart} type="date" value={periodEnd} onChange={(event) => setPeriodEnd(event.target.value)} /></label></div> : <p className="mt-2 text-sm text-slate-500">Hiển thị toàn bộ công việc chưa chốt, không giới hạn thời gian.</p>}
      </section>

      <label className="panel block p-4 text-sm font-semibold text-slate-700 md:hidden">
        Chọn ca cần đối soát
        <select className="mt-2 min-h-11 w-full rounded-xl border-slate-300 bg-white px-3" onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setSelectedId(event.target.value || null)} value={selectedId ?? ""}>
          <option value="">Chọn ca làm việc</option>
          {pageItems.map((item: ScopedStaffingReconcile): React.JSX.Element => <option key={item.assignment_id} value={item.assignment_id}>{item.employee_name} · {item.customer_name} · {statusLabel(item.reconciliation_status)}</option>)}
        </select>
        {pageItems.length === 0 ? <p className="mt-2 font-normal text-slate-500">Chưa có ca được phân công.</p> : null}
      </label>
      <ReconciliationPagination className="panel rounded-2xl md:hidden" currentItemCount={pageItems.length} currentPage={currentPage} hasNextPage={hasNextPage} nextPagePending={query.isFetchingNextPage} onPageChange={changePage} />

      <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(280px,0.72fr)_minmax(0,1.28fr)]">
      <section className="panel hidden overflow-hidden md:block">
        <div className="border-b border-slate-200 px-5 py-4"><h2 className="font-bold text-slate-950">{collection === "pending" ? "Ca cần đối soát" : "Ca đã xác nhận"}</h2><p className="mt-1 text-sm text-slate-500">Mỗi dòng là một nhân viên tại nơi làm việc.</p></div>
        <div className="max-h-[70vh] divide-y divide-slate-100 overflow-y-auto">
          {pageItems.map((item: ScopedStaffingReconcile): React.JSX.Element => <button aria-pressed={selectedId === item.assignment_id} className={`w-full px-5 py-4 text-left hover:bg-slate-50 ${selectedId === item.assignment_id ? "bg-blue-50" : ""}`} key={item.assignment_id} onClick={(): void => setSelectedId(item.assignment_id)} type="button"><div className="flex items-start justify-between gap-2"><div><p className="font-bold text-slate-900">{item.employee_name}</p><p className="mt-1 text-xs text-slate-500">{item.customer_name} · {scope.branches.find((branch): boolean => branch.id === item.branch_id)?.name ?? "Chi nhánh"}</p></div><span className={`shrink-0 rounded-full px-2.5 py-1 text-[11px] font-bold ${statusTone(item.reconciliation_status)}`}>{statusLabel(item.reconciliation_status)}</span></div><p className="mt-2 text-xs text-slate-500">{formatDateTime(item.scheduled_starts_at)}</p></button>)}
          {pageItems.length === 0 ? <p className="p-8 text-center text-sm text-slate-500">Chưa có ca được phân công.</p> : null}
        </div>
        <ReconciliationPagination currentItemCount={pageItems.length} currentPage={currentPage} hasNextPage={hasNextPage} nextPagePending={query.isFetchingNextPage} onPageChange={changePage} />
      </section>

      {selected ? <div className="min-w-0 space-y-5">
        {message ? <div className="rounded-xl border border-blue-200 bg-blue-50 px-4 py-3 text-sm font-medium text-blue-800">{message}</div> : null}
        <section className="panel p-4 sm:p-6">
          <div className="flex flex-wrap items-start justify-between gap-3"><div><h2 className="text-lg font-black text-slate-950">{selected.employee_name}</h2><p className="mt-1 text-sm text-slate-500">{selected.employee_code} · {selected.customer_name}</p></div><span className={`rounded-full px-3 py-1 text-xs font-bold ${statusTone(selected.reconciliation_status)}`}>{statusLabel(selected.reconciliation_status)}</span></div>
          <div className="mt-5 grid gap-3 md:grid-cols-3">
            <div className="rounded-xl bg-violet-50 p-4"><p className="text-xs font-bold uppercase text-violet-600">Nhân viên ghi</p><p className="mt-2 font-black text-violet-950">{selected.customer_name}</p><p className="mt-1 text-sm font-bold text-violet-900">{formatDuration(selected.staff_worked_seconds)}</p><p className="mt-1 text-xs leading-5 text-violet-700">{selected.staff_started_at ? formatDateTime(selected.staff_started_at) : "Chưa bắt đầu"} → {selected.staff_ended_at ? formatDateTime(selected.staff_ended_at) : "đang làm"}</p></div>
            <div className="rounded-xl bg-amber-50 p-4"><p className="text-xs font-bold uppercase text-amber-600">Khách hàng xác nhận</p><p className="mt-2 font-black text-amber-950">{selected.confirmed_customer_name ?? "Chưa nhập"}</p><p className="mt-1 text-sm font-bold text-amber-900">{selected.customer_record ? formatDuration(selected.customer_record.confirmed_worked_seconds) : "—"}</p><p className="mt-1 text-xs leading-5 text-amber-700">{selected.customer_record ? `${formatDateTime(selected.customer_record.confirmed_started_at)} → ${formatDateTime(selected.customer_record.confirmed_ended_at)}` : "Chưa có thời gian khách hàng xác nhận"}</p>{selected.customer_record?.customer_reference ? <p className="mt-1 text-xs text-amber-700">Mã bill: {selected.customer_record.customer_reference}</p> : null}</div>
            <div className="rounded-xl bg-emerald-50 p-4"><p className="text-xs font-bold uppercase text-emerald-600">Kết quả cuối</p><p className="mt-2 text-xl font-black text-emerald-950">{selected.final_worked_seconds ? formatDuration(selected.final_worked_seconds) : "Chưa chốt"}</p><p className="mt-1 text-xs text-emerald-700">Dùng cho thanh toán</p></div>
          </div>
        </section>

        <form className="panel p-4 sm:p-6" onSubmit={saveEvidence}>
          <h3 className="font-bold text-slate-950">Xác nhận / bill từ khách hàng</h3><p className="mt-1 text-sm text-slate-500">{selected.customer_record ? "Nhập đúng dữ liệu khách hàng cung cấp, không sửa dữ liệu nhân viên." : "Hệ thống đã sao chép giờ nhân viên vào bản nháp. Hãy kiểm tra rồi bấm lưu để xác nhận; dữ liệu chưa được lưu tự động."}</p>
          {selected.staff_started_at && selected.staff_ended_at ? <button className="mt-3 inline-flex min-h-10 w-full items-center justify-center gap-2 rounded-xl border border-violet-200 bg-violet-50 px-3 text-sm font-bold text-violet-800 hover:bg-violet-100 sm:w-auto" disabled={!canManage && !canCorrect} onClick={copyStaffEvidence} type="button"><RefreshCw className="size-4" />Sao chép lại dữ liệu nhân viên</button> : null}
          <label className="mt-4 block text-sm font-semibold text-slate-700">Khách hàng / nơi làm việc xác nhận
            <select
              className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
              disabled={(!canManage && !canCorrect) || customersQuery.isPending}
              required
              value={evidence.customerId}
              onChange={(event) => setEvidence({ ...evidence, customerId: event.target.value })}
            >
              <option value="">Chọn đúng khách hàng trên dữ liệu xác nhận</option>
              {customers.map((customer) => (
                <option key={customer.id} value={customer.id}>{customer.name}</option>
              ))}
            </select>
          </label>
          <div className="mt-4 grid min-w-0 gap-3 sm:grid-cols-2"><label className="min-w-0 text-sm font-semibold text-slate-700">Bắt đầu xác nhận<input className="mt-1.5 min-w-0 w-full max-w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={!canManage && !canCorrect} required step="1" type="datetime-local" value={evidence.startedAt} onChange={(event) => setEvidence({ ...evidence, startedAt: event.target.value, startedAtExact: null })} /></label><label className="min-w-0 text-sm font-semibold text-slate-700">Kết thúc xác nhận<input className="mt-1.5 min-w-0 w-full max-w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={!canManage && !canCorrect} required step="1" type="datetime-local" value={evidence.endedAt} onChange={(event) => setEvidence({ ...evidence, endedAt: event.target.value, endedAtExact: null })} /></label></div>
          <label className="mt-3 block text-sm font-semibold text-slate-700">Mã bill / tham chiếu<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={!canManage && !canCorrect} value={evidence.reference} onChange={(event) => setEvidence({ ...evidence, reference: event.target.value })} /></label>
          <label className="mt-3 block text-sm font-semibold text-slate-700">Ghi chú<textarea className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={!canManage && !canCorrect} rows={2} value={evidence.notes} onChange={(event) => setEvidence({ ...evidence, notes: event.target.value })} /></label>
          {selected.assignment_status !== "approved" || canCorrect ? <button className="action-secondary mt-4 w-full sm:w-auto" disabled={(!canManage && !canCorrect) || evidenceMutation.isPending} type="submit"><Save className="size-4" />{selected.assignment_status === "approved" ? "Lưu bản sửa bằng chứng" : "Lưu bằng chứng khách hàng"}</button> : null}
        </form>

        <section className="panel p-4 sm:p-6"><div className="flex items-center gap-2"><GitCompareArrows className="size-5 text-blue-600" /><h3 className="font-bold text-slate-950">Chốt đối soát</h3></div>
          {selected.assignment_status !== "approved" && selected.reconciliation_status === "matched" ? <div className="mt-4 rounded-2xl border border-emerald-200 bg-emerald-50 p-4"><p className="font-bold text-emerald-950">Hai nguồn đã khớp</p><p className="mt-1 text-sm leading-6 text-emerald-800">Xác nhận kết quả khi giờ nhân viên và dữ liệu khách hàng đã khớp hoàn toàn. Thao tác này không thay đổi dữ liệu của hai bên.</p><button className="mt-3 inline-flex min-h-11 w-full items-center justify-center gap-2 rounded-xl bg-emerald-700 px-4 text-sm font-bold text-white hover:bg-emerald-800 disabled:cursor-not-allowed disabled:opacity-50 sm:w-auto" disabled={!canManage || acceptStaffRecordMutation.isPending || evidenceMutation.isPending || reconcileMutation.isPending} onClick={(): void => { if (window.confirm("Xác nhận hai nguồn đã khớp và khóa kết quả ca làm này?")) acceptStaffRecordMutation.mutate(); }} type="button"><CheckCircle2 className="size-4" />{acceptStaffRecordMutation.isPending ? "Đang xác nhận..." : "Xác nhận giờ nhân viên"}</button></div> : null}
          {selected.assignment_status !== "approved" && selected.reconciliation_status !== "matched" ? <p className="mt-4 rounded-xl bg-amber-50 px-4 py-3 text-sm text-amber-800">Nhập dữ liệu khách hàng trước. Nút xác nhận nhanh chỉ xuất hiện khi khách hàng và nhân viên ghi nhận hoàn toàn giống nhau.</p> : null}
          {selected.assignment_status !== "approved" ? <div className="mt-4 grid min-w-0 gap-3 sm:grid-cols-2">
            <label className="min-w-0 text-sm font-semibold text-slate-700">Khách hàng kết luận
              <select className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5" required value={finalCustomerId} onChange={(event) => setFinalCustomerId(event.target.value)}>
                <option value="">Chọn khách hàng cuối cùng</option>
                {customers.filter((customer) => customer.status === "active").map((customer) => <option key={customer.id} value={customer.id}>{customer.name}</option>)}
              </select>
              {customersQuery.hasNextPage ? <button className="mt-2 text-xs font-semibold text-blue-700" disabled={customersQuery.isFetchingNextPage} onClick={() => void customersQuery.fetchNextPage()} type="button">{customersQuery.isFetchingNextPage ? "Đang tải..." : "Tải thêm khách hàng"}</button> : null}
            </label>
            <label className="min-w-0 text-sm font-semibold text-slate-700">Công việc kết luận
              <select className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5" required value={finalJobId} onChange={(event) => setFinalJobId(event.target.value)}>
                <option value="">Chọn công việc cuối cùng</option>
                {jobs.filter((job) => job.status === "active").map((job) => <option key={job.id} value={job.id}>{job.name}</option>)}
              </select>
              {jobsQuery.hasNextPage ? <button className="mt-2 text-xs font-semibold text-blue-700" disabled={jobsQuery.isFetchingNextPage} onClick={() => void jobsQuery.fetchNextPage()} type="button">{jobsQuery.isFetchingNextPage ? "Đang tải..." : "Tải thêm công việc"}</button> : null}
            </label>
          </div> : null}
          <div className="mt-4 grid min-w-0 gap-3 sm:grid-cols-2"><label className="min-w-0 text-sm font-semibold text-slate-700">Thời gian cuối (giờ)<input className="mt-1.5 min-w-0 w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={selected.assignment_status === "approved" && !canCorrect} min="0.0001" required step="0.0001" type="number" value={finalHours} onChange={(event) => setFinalHours(event.target.value)} /></label>{selected.assignment_status === "approved" ? <label className="min-w-0 text-sm font-semibold text-slate-700">Lý do điều chỉnh<input className="mt-1.5 min-w-0 w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={!canCorrect} placeholder="Bắt buộc, ít nhất 3 ký tự" value={correctionReason} onChange={(event) => setCorrectionReason(event.target.value)} /></label> : <label className="min-w-0 text-sm font-semibold text-slate-700">Lý do xử lý chênh lệch<input className="mt-1.5 min-w-0 w-full rounded-xl border border-slate-200 px-3 py-2.5" placeholder="Bắt buộc nếu hai nguồn không khớp hoặc thay đổi kết luận" value={resolution} onChange={(event) => setResolution(event.target.value)} /></label>}</div>
          {selected.assignment_status === "approved" ? <div className="mt-4"><p className="flex items-center gap-2 text-sm font-semibold text-emerald-700"><CheckCircle2 className="size-5" />Đã chốt · phiên bản {selected.result_revision_number ?? 1}. Mỗi lần sửa tạo một phiên bản mới.</p>{canCorrect ? <button className="action-primary mt-3 w-full sm:w-auto" disabled={!selected.result_revision_id || correctionReason.trim().length < 3 || correctionMutation.isPending} onClick={() => correctionMutation.mutate()} type="button"><Save className="size-4" />Lưu phiên bản điều chỉnh</button> : null}</div> : <button className="action-primary mt-4 w-full sm:w-auto" disabled={!canManage || !selected.customer_record || selected.staff_worked_seconds <= 0 || !finalCustomerId || !finalJobId || reconcileMutation.isPending} onClick={() => reconcileMutation.mutate()} type="button"><CheckCircle2 className="size-4" />Chốt kết quả</button>}
        </section>
      </div> : null}
      </div>
    </div>
  );
}
