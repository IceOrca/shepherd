import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, CircleAlert, GitCompareArrows, RefreshCw, Save } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import type {
  Customer,
  ReconciliationStatus,
  StaffingReconciliation,
  StaffingReconciliationPageResponse,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { formatDateTime, formatDuration } from "../../shared/lib/format";
import { useAuth } from "../auth/AuthProvider";
import {
  acceptAssignmentStaffRecordForBranch,
  listCustomersForBranch,
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
  loadReconciliationScopePage,
  type ReconciliationScopeCursor,
  type ReconciliationScopePage,
} from "./reconciliationCursor";

interface ScopedStaffingReconciliation extends StaffingReconciliation {
  branch_id: string;
}

function compareScopedReconciliations(
  left: ScopedStaffingReconciliation,
  right: ScopedStaffingReconciliation,
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
  return new Date(date.getTime() - offset).toISOString().slice(0, 16);
}

function statusLabel(status: ReconciliationStatus): string {
  const labels: Record<ReconciliationStatus, string> = {
    pending_staff: "Chờ nhân viên kết thúc",
    pending_customer: "Chờ dữ liệu khách hàng",
    matched: "Khớp",
    discrepancy: "Chênh lệch",
    reconciled: "Đã chốt",
  };
  return labels[status];
}

function statusTone(status: ReconciliationStatus): string {
  if (status === "matched" || status === "reconciled") return "bg-emerald-50 text-emerald-700";
  if (status === "discrepancy") return "bg-red-50 text-red-700";
  return "bg-amber-50 text-amber-700";
}

interface EvidenceDraft {
  customerId: string;
  startedAt: string;
  endedAt: string;
  reference: string;
  notes: string;
}

export function ReconciliationPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const scope: ReturnType<typeof useOperationsScope> = useOperationsScope();
  const queryClient: ReturnType<typeof useQueryClient> = useQueryClient();
  const canRead: boolean = auth.profile?.permissions.includes("business.reconciliation.read") ?? false;
  const canManage: boolean = auth.profile?.permissions.includes("business.reconciliation.manage") ?? false;
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [evidence, setEvidence] = useState<EvidenceDraft>({
    customerId: "",
    startedAt: "",
    endedAt: "",
    reference: "",
    notes: "",
  });
  const [finalHours, setFinalHours] = useState("");
  const [resolution, setResolution] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  const query = useInfiniteQuery({
    queryKey: [
      ...operationsQueryKeys.reconciliations,
      "scope",
      scope.scopeKey,
      "customer",
      scope.selectedCustomerId ?? "all",
    ],
    initialPageParam: createReconciliationScopeCursor<ScopedStaffingReconciliation>(scope.branchIds),
    queryFn: ({ pageParam }: { pageParam: ReconciliationScopeCursor<ScopedStaffingReconciliation> }): Promise<ReconciliationScopePage<ScopedStaffingReconciliation>> =>
      loadReconciliationScopePage<ScopedStaffingReconciliation>({
        cursor: pageParam,
        fetchBranchPage: async (
          branchId: string,
          cursor: string | null,
        ): Promise<StaffingReconciliationPageResponse & { items: ScopedStaffingReconciliation[] }> => {
          const page: StaffingReconciliationPageResponse = await listReconciliationsForBranch(
            branchId,
            cursor,
            scope.selectedCustomerId,
          );
          return {
            ...page,
            items: page.items.map(
              (item: StaffingReconciliation): ScopedStaffingReconciliation => ({ ...item, branch_id: branchId }),
            ),
          };
        },
        compare: compareScopedReconciliations,
        itemKey: (item: ScopedStaffingReconciliation): string => item.assignment_id,
      }),
    getNextPageParam: (
      lastPage: ReconciliationScopePage<ScopedStaffingReconciliation>,
    ): ReconciliationScopeCursor<ScopedStaffingReconciliation> | undefined => lastPage.nextCursor ?? undefined,
    enabled: canRead && scope.branchIds.length > 0,
  });
  const loadedPages: ReconciliationScopePage<ScopedStaffingReconciliation>[] = query.data?.pages ?? [];
  const pageItems: ScopedStaffingReconciliation[] = loadedPages[currentPage - 1]?.items ?? [];
  const hasNextPage: boolean = currentPage < loadedPages.length || query.hasNextPage;
  const selected: ScopedStaffingReconciliation | null =
    pageItems.find(
      (item: ScopedStaffingReconciliation): boolean => item.assignment_id === selectedId,
    ) ?? null;

  const customersQuery = useQuery<Customer[], Error>({
    queryKey: [...operationsQueryKeys.customers, "branch", selected?.branch_id ?? "none"],
    queryFn: (): Promise<Customer[]> => listCustomersForBranch(selected?.branch_id ?? ""),
    enabled: canRead && selected !== null,
  });

  useEffect((): void => {
    const selectionRemainsVisible: boolean = pageItems.some(
      (item: ScopedStaffingReconciliation): boolean => item.assignment_id === selectedId,
    );
    if (!selectionRemainsVisible) {
      setSelectedId(pageItems.at(0)?.assignment_id ?? null);
    }
  }, [pageItems, selectedId]);

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
    setEvidence({
      customerId: selected.customer_record?.confirmed_customer_id ?? "",
      startedAt: localDateTime(selected.customer_record?.confirmed_started_at),
      endedAt: localDateTime(selected.customer_record?.confirmed_ended_at),
      reference: selected.customer_record?.customer_reference ?? "",
      notes: selected.customer_record?.notes ?? "",
    });
    const seconds = selected.final_worked_seconds ?? selected.customer_record?.confirmed_worked_seconds ?? selected.staff_worked_seconds;
    setFinalHours(seconds > 0 ? (seconds / 3600).toFixed(2) : "");
    setResolution(selected.adjustment_reason ?? "");
  }, [selected]);

  const refresh = () => queryClient.invalidateQueries({ queryKey: operationsQueryKeys.reconciliations });
  const evidenceMutation = useMutation({
    mutationFn: () => saveCustomerWorkRecordForBranch(selected?.branch_id ?? "", selectedId ?? "", {
      confirmed_customer_id: evidence.customerId,
      confirmed_started_at: new Date(evidence.startedAt).toISOString(),
      confirmed_ended_at: new Date(evidence.endedAt).toISOString(),
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

  const saveEvidence = (event: FormEvent) => {
    event.preventDefault();
    setMessage(null);
    evidenceMutation.mutate();
  };

  if (!canRead) return <section className="panel p-8 text-center text-sm text-slate-500">Bạn chưa có quyền xem đối soát.</section>;
  if (query.isPending) return <section className="panel p-8 text-center text-sm text-slate-500"><RefreshCw className="mr-2 inline size-4 animate-spin" />Đang tải dữ liệu đối soát...</section>;
  if (query.error) return <section className="panel p-8 text-center text-sm text-red-600"><CircleAlert className="mr-2 inline size-4" />{friendlyApiError(query.error, "Không thể tải đối soát.")}</section>;

  return (
    <div className="space-y-4">
      <ReconciliationModeSelector mode="planned" />

      <label className="panel block p-4 text-sm font-semibold text-slate-700 md:hidden">
        Chọn ca cần đối soát
        <select className="mt-2 min-h-11 w-full rounded-xl border-slate-300 bg-white px-3" onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setSelectedId(event.target.value || null)} value={selectedId ?? ""}>
          <option value="">Chọn ca làm việc</option>
          {pageItems.map((item: ScopedStaffingReconciliation): React.JSX.Element => <option key={item.assignment_id} value={item.assignment_id}>{item.employee_name} · {item.customer_name} · {statusLabel(item.reconciliation_status)}</option>)}
        </select>
        {pageItems.length === 0 ? <p className="mt-2 font-normal text-slate-500">Chưa có ca được phân công.</p> : null}
      </label>
      <ReconciliationPagination className="panel rounded-2xl md:hidden" currentItemCount={pageItems.length} currentPage={currentPage} hasNextPage={hasNextPage} nextPagePending={query.isFetchingNextPage} onPageChange={changePage} />

      <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(280px,0.72fr)_minmax(0,1.28fr)]">
      <section className="panel hidden overflow-hidden md:block">
        <div className="border-b border-slate-200 px-5 py-4"><h2 className="font-bold text-slate-950">Ca cần đối soát</h2><p className="mt-1 text-sm text-slate-500">So sánh dữ liệu nhân viên với xác nhận khách hàng.</p></div>
        <div className="max-h-[70vh] divide-y divide-slate-100 overflow-y-auto">
          {pageItems.map((item: ScopedStaffingReconciliation): React.JSX.Element => <button aria-pressed={selectedId === item.assignment_id} className={`w-full px-5 py-4 text-left hover:bg-slate-50 ${selectedId === item.assignment_id ? "bg-blue-50" : ""}`} key={item.assignment_id} onClick={(): void => setSelectedId(item.assignment_id)} type="button"><div className="flex items-start justify-between gap-2"><div><p className="font-bold text-slate-900">{item.employee_name}</p><p className="mt-1 text-xs text-slate-500">{item.customer_name} · {scope.branches.find((branch): boolean => branch.id === item.branch_id)?.name ?? "Chi nhánh"}</p></div><span className={`shrink-0 rounded-full px-2.5 py-1 text-[11px] font-bold ${statusTone(item.reconciliation_status)}`}>{statusLabel(item.reconciliation_status)}</span></div><p className="mt-2 text-xs text-slate-500">{formatDateTime(item.scheduled_starts_at)}</p></button>)}
          {pageItems.length === 0 ? <p className="p-8 text-center text-sm text-slate-500">Chưa có ca được phân công.</p> : null}
        </div>
        <ReconciliationPagination currentItemCount={pageItems.length} currentPage={currentPage} hasNextPage={hasNextPage} nextPagePending={query.isFetchingNextPage} onPageChange={changePage} />
      </section>

      {selected ? <div className="min-w-0 space-y-5">
        {message ? <div className="rounded-xl border border-blue-200 bg-blue-50 px-4 py-3 text-sm font-medium text-blue-800">{message}</div> : null}
        <section className="panel p-4 sm:p-6">
          <div className="flex flex-wrap items-start justify-between gap-3"><div><h2 className="text-lg font-black text-slate-950">{selected.employee_name}</h2><p className="mt-1 text-sm text-slate-500">{selected.employee_code} · {selected.customer_name}</p></div><span className={`rounded-full px-3 py-1 text-xs font-bold ${statusTone(selected.reconciliation_status)}`}>{statusLabel(selected.reconciliation_status)}</span></div>
          <div className="mt-5 grid gap-3 md:grid-cols-3">
            <div className="rounded-xl bg-violet-50 p-4"><p className="text-xs font-bold uppercase text-violet-600">Nhân viên ghi</p><p className="mt-2 text-xl font-black text-violet-950">{formatDuration(selected.staff_worked_seconds)}</p><p className="mt-1 text-xs text-violet-700">{selected.staff_started_at ? formatDateTime(selected.staff_started_at) : "Chưa bắt đầu"}</p></div>
            <div className="rounded-xl bg-amber-50 p-4"><p className="text-xs font-bold uppercase text-amber-600">Khách hàng xác nhận</p><p className="mt-2 text-xl font-black text-amber-950">{selected.customer_record ? formatDuration(selected.customer_record.confirmed_worked_seconds) : "—"}</p><p className="mt-1 text-xs text-amber-700">Theo xác nhận khách hàng</p></div>
            <div className="rounded-xl bg-emerald-50 p-4"><p className="text-xs font-bold uppercase text-emerald-600">Kết quả cuối</p><p className="mt-2 text-xl font-black text-emerald-950">{selected.final_worked_seconds ? formatDuration(selected.final_worked_seconds) : "Chưa chốt"}</p><p className="mt-1 text-xs text-emerald-700">Dùng cho thanh toán</p></div>
          </div>
        </section>

        <form className="panel p-4 sm:p-6" onSubmit={saveEvidence}>
          <h3 className="font-bold text-slate-950">Xác nhận / bill từ khách hàng</h3><p className="mt-1 text-sm text-slate-500">Nhập đúng dữ liệu khách hàng cung cấp, không sửa dữ liệu nhân viên.</p>
          <label className="mt-4 block text-sm font-semibold text-slate-700">Khách hàng / nơi làm việc xác nhận
            <select
              className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
              disabled={!canManage || selected.assignment_status === "approved" || customersQuery.isPending}
              required
              value={evidence.customerId}
              onChange={(event) => setEvidence({ ...evidence, customerId: event.target.value })}
            >
              <option value="">Chọn đúng khách hàng trên dữ liệu xác nhận</option>
              {(customersQuery.data ?? []).map((customer) => (
                <option key={customer.id} value={customer.id}>{customer.name}</option>
              ))}
            </select>
          </label>
          <div className="mt-4 grid min-w-0 gap-3 sm:grid-cols-2"><label className="min-w-0 text-sm font-semibold text-slate-700">Bắt đầu xác nhận<input className="mt-1.5 min-w-0 w-full max-w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={!canManage || selected.assignment_status === "approved"} required type="datetime-local" value={evidence.startedAt} onChange={(event) => setEvidence({ ...evidence, startedAt: event.target.value })} /></label><label className="min-w-0 text-sm font-semibold text-slate-700">Kết thúc xác nhận<input className="mt-1.5 min-w-0 w-full max-w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={!canManage || selected.assignment_status === "approved"} required type="datetime-local" value={evidence.endedAt} onChange={(event) => setEvidence({ ...evidence, endedAt: event.target.value })} /></label></div>
          <label className="mt-3 block text-sm font-semibold text-slate-700">Mã bill / tham chiếu<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={!canManage || selected.assignment_status === "approved"} value={evidence.reference} onChange={(event) => setEvidence({ ...evidence, reference: event.target.value })} /></label>
          <label className="mt-3 block text-sm font-semibold text-slate-700">Ghi chú<textarea className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={!canManage || selected.assignment_status === "approved"} rows={2} value={evidence.notes} onChange={(event) => setEvidence({ ...evidence, notes: event.target.value })} /></label>
          {selected.assignment_status !== "approved" ? <button className="action-secondary mt-4 w-full sm:w-auto" disabled={!canManage || evidenceMutation.isPending} type="submit"><Save className="size-4" />Lưu xác nhận khách hàng</button> : null}
        </form>

        <section className="panel p-4 sm:p-6"><div className="flex items-center gap-2"><GitCompareArrows className="size-5 text-blue-600" /><h3 className="font-bold text-slate-950">Chốt đối soát</h3></div>
          {selected.assignment_status !== "approved" && selected.reconciliation_status === "matched" ? <div className="mt-4 rounded-2xl border border-emerald-200 bg-emerald-50 p-4"><p className="font-bold text-emerald-950">Hai nguồn đã khớp</p><p className="mt-1 text-sm leading-6 text-emerald-800">Xác nhận kết quả khi giờ nhân viên và dữ liệu khách hàng đã khớp hoàn toàn. Thao tác này không thay đổi dữ liệu của hai bên.</p><button className="mt-3 inline-flex min-h-11 w-full items-center justify-center gap-2 rounded-xl bg-emerald-700 px-4 text-sm font-bold text-white hover:bg-emerald-800 disabled:cursor-not-allowed disabled:opacity-50 sm:w-auto" disabled={!canManage || acceptStaffRecordMutation.isPending || evidenceMutation.isPending || reconcileMutation.isPending} onClick={(): void => { if (window.confirm("Xác nhận hai nguồn đã khớp và khóa kết quả ca làm này?")) acceptStaffRecordMutation.mutate(); }} type="button"><CheckCircle2 className="size-4" />{acceptStaffRecordMutation.isPending ? "Đang xác nhận..." : "Xác nhận giờ nhân viên"}</button></div> : null}
          {selected.assignment_status !== "approved" && selected.reconciliation_status !== "matched" ? <p className="mt-4 rounded-xl bg-amber-50 px-4 py-3 text-sm text-amber-800">Nhập dữ liệu khách hàng trước. Nút xác nhận nhanh chỉ xuất hiện khi khách hàng và nhân viên ghi nhận hoàn toàn giống nhau.</p> : null}
          <div className="mt-4 grid min-w-0 gap-3 sm:grid-cols-2"><label className="min-w-0 text-sm font-semibold text-slate-700">Thời gian cuối (giờ)<input className="mt-1.5 min-w-0 w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={selected.assignment_status === "approved"} min="0.01" required step="0.01" type="number" value={finalHours} onChange={(event) => setFinalHours(event.target.value)} /></label><label className="min-w-0 text-sm font-semibold text-slate-700">Lý do xử lý chênh lệch<input className="mt-1.5 min-w-0 w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={selected.assignment_status === "approved"} placeholder="Bắt buộc nếu hai nguồn không khớp" value={resolution} onChange={(event) => setResolution(event.target.value)} /></label></div>
          {selected.assignment_status === "approved" ? <p className="mt-4 flex items-center gap-2 text-sm font-semibold text-emerald-700"><CheckCircle2 className="size-5" />Kết quả đã khóa và sẵn sàng cho nghiệp vụ thanh toán.</p> : <button className="action-primary mt-4 w-full sm:w-auto" disabled={!canManage || !selected.customer_record || selected.staff_worked_seconds <= 0 || reconcileMutation.isPending} onClick={() => reconcileMutation.mutate()} type="button"><CheckCircle2 className="size-4" />Chốt kết quả</button>}
        </section>
      </div> : null}
      </div>
    </div>
  );
}
