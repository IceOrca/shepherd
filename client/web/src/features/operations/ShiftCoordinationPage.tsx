import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CalendarPlus2, CircleAlert, RefreshCw, UserPlus, UsersRound } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import type {
  Customer,
  ShiftAssignment,
  StaffingCandidate,
  StaffingShift,
  StaffingShiftCreateRequest,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { formatDateTime, shiftStatusLabel } from "../../shared/lib/format";
import { useAuth } from "../auth/AuthProvider";
import {
  createShiftAssignmentForBranch,
  createStaffingShift,
  listCustomers,
  listJobs,
  listShiftCandidatesForBranch,
  listStaffingShiftsForBranch,
  operationsQueryKeys,
} from "./api";
import {
  useOperationsScope,
  type OperationsScopeCustomer,
} from "./OperationsScopeProvider";

interface ScopedStaffingShift extends StaffingShift {
  branch_id: string;
}

interface AssignShiftInput {
  branchId: string;
  shiftId: string;
  employeeId: string;
}

import {
  createReconciliationScopeCursor,
  loadReconcileScopePage,
  type ReconcileScopePage,
  type ReconciliationScopeCursor,
} from "./reconciliationCursor";

function compareScopedShifts(left: ScopedStaffingShift, right: ScopedStaffingShift): number {
  const timeOrder: number = right.starts_at.localeCompare(left.starts_at);
  return timeOrder !== 0 ? timeOrder : right.id.localeCompare(left.id);
}

interface ShiftDraft {
  customerId: string;
  jobId: string;
  startsAt: string;
  endsAt: string;
  requiredWorkers: string;
  notes: string;
}

const emptyDraft: ShiftDraft = {
  customerId: "",
  jobId: "",
  startsAt: "",
  endsAt: "",
  requiredWorkers: "1",
  notes: "",
};

export function ShiftCoordinationPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const scope: ReturnType<typeof useOperationsScope> = useOperationsScope();
  const queryClient: ReturnType<typeof useQueryClient> = useQueryClient();
  const canManage: boolean = auth.profile?.permissions.includes("business.shifts.manage") ?? false;
  const [draft, setDraft] = useState<ShiftDraft>(emptyDraft);
  const [selectedShiftId, setSelectedShiftId] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const customersQuery = useInfiniteQuery({
    queryKey: operationsQueryKeys.customers,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => listCustomers(pageParam),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
  const jobsQuery = useInfiniteQuery({
    queryKey: operationsQueryKeys.jobs,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => listJobs(pageParam),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
  const shiftsQuery = useInfiniteQuery({
    queryKey: [...operationsQueryKeys.shifts, "scope", scope.scopeKey],
    initialPageParam: createReconciliationScopeCursor<ScopedStaffingShift>(scope.branchIds),
    queryFn: ({ pageParam }: { pageParam: ReconciliationScopeCursor<ScopedStaffingShift> }) =>
      loadReconcileScopePage({
        cursor: pageParam,
        fetchBranchPage: async (branchId: string, cursor: string | null) => {
          const page = await listStaffingShiftsForBranch(branchId, cursor);
          return {
            ...page,
            items: page.items.map((shift): ScopedStaffingShift => ({ ...shift, branch_id: branchId })),
          };
        },
        compare: compareScopedShifts,
        itemKey: (shift: ScopedStaffingShift): string => shift.id,
      }),
    getNextPageParam: (page: ReconcileScopePage<ScopedStaffingShift>) => page.nextCursor ?? undefined,
    enabled: scope.branchIds.length > 0,
  });
  const loadedShifts: ScopedStaffingShift[] =
    shiftsQuery.data?.pages.flatMap((page: ReconcileScopePage<ScopedStaffingShift>) => page.items) ?? [];
  const jobs = jobsQuery.data?.pages.flatMap((page) => page.items) ?? [];
  const shifts: ScopedStaffingShift[] = useMemo<ScopedStaffingShift[]>(
    (): ScopedStaffingShift[] =>
      [...loadedShifts]
        .filter(
          (shift: ScopedStaffingShift): boolean =>
            scope.selectedCustomerId === null || shift.customer_id === scope.selectedCustomerId,
        )
        .sort(
          (left: ScopedStaffingShift, right: ScopedStaffingShift): number =>
            new Date(left.starts_at).getTime() - new Date(right.starts_at).getTime(),
        ),
    [scope.selectedCustomerId, loadedShifts],
  );
  const selectedShift: ScopedStaffingShift | null =
    shifts.find((shift: ScopedStaffingShift): boolean => shift.id === selectedShiftId) ?? null;
  const candidatesQuery = useInfiniteQuery({
    queryKey: [
      ...operationsQueryKeys.candidates(selectedShiftId ?? ""),
      selectedShift?.branch_id ?? "none",
    ],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      listShiftCandidatesForBranch(
        selectedShift?.branch_id ?? "",
        selectedShiftId ?? "",
        pageParam,
      ),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    enabled: selectedShift !== null,
  });
  const candidates: StaffingCandidate[] =
    candidatesQuery.data?.pages.flatMap((page) => page.items) ?? [];

  const customerNames: Map<string, string> = useMemo<Map<string, string>>(
    (): Map<string, string> =>
      new Map(
        scope.customers.map(
          (customer: OperationsScopeCustomer): [string, string] => [
            customer.customer_id,
            customer.customer_name,
          ],
        ),
      ),
    [scope.customers],
  );

  useEffect((): void => {
    if (selectedShiftId !== null && selectedShift === null) {
      setSelectedShiftId(null);
    }
  }, [selectedShift, selectedShiftId]);

  const createMutation = useMutation({
    mutationFn: createStaffingShift,
    onSuccess: (shift) => {
      setDraft(emptyDraft);
      setSelectedShiftId(shift.id);
      setMessage("Đã tạo yêu cầu ca. Hãy chọn nhân viên phù hợp ở danh sách bên dưới.");
      void queryClient.invalidateQueries({ queryKey: operationsQueryKeys.shifts });
    },
    onError: (error) => setMessage(friendlyApiError(error, "Không thể tạo ca làm.")),
  });

  const assignMutation = useMutation({
    mutationFn: ({ branchId, shiftId, employeeId }: AssignShiftInput): Promise<ShiftAssignment> =>
      createShiftAssignmentForBranch(branchId, shiftId, { employee_id: employeeId }),
    onSuccess: (): void => {
      setMessage("Đã phân công nhân viên vào ca.");
      void queryClient.invalidateQueries({ queryKey: operationsQueryKeys.all });
    },
    onError: (error) =>
      setMessage(
        friendlyApiError(error, "Không thể phân công. Hãy kiểm tra đơn giá, vị trí công việc và lịch trùng."),
      ),
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setMessage(null);
    const payload: StaffingShiftCreateRequest = {
      customer_id: draft.customerId,
      job_id: draft.jobId,
      starts_at: new Date(draft.startsAt).toISOString(),
      ends_at: new Date(draft.endsAt).toISOString(),
      required_workers: Number(draft.requiredWorkers),
      notes: draft.notes.trim() || null,
    };
    createMutation.mutate(payload);
  };

  return (
    <div className={`grid gap-5 ${canManage ? "xl:grid-cols-[minmax(320px,0.8fr)_minmax(0,1.2fr)]" : "grid-cols-1"}`}>
      {canManage ? <section className="panel p-5 sm:p-6">
        <div className="flex items-center gap-3">
          <div className="grid size-10 place-items-center rounded-xl bg-blue-50 text-blue-700"><CalendarPlus2 className="size-5" /></div>
          <div><h2 className="font-bold text-slate-950">Tạo ca theo yêu cầu khách hàng</h2><p className="text-sm text-slate-500">Địa điểm và công việc được khóa theo ca.</p></div>
        </div>
        <form className="mt-5 space-y-4" onSubmit={submit}>
          <label className="block text-sm font-semibold text-slate-700">Khách hàng
            <select className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" required value={draft.customerId} onChange={(event) => setDraft({ ...draft, customerId: event.target.value })}>
              <option value="">Chọn khách hàng</option>{customersQuery.data?.pages.flatMap((page) => page.items).map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
            {customersQuery.hasNextPage ? <button className="mt-2 text-xs font-semibold text-blue-700" disabled={customersQuery.isFetchingNextPage} onClick={() => void customersQuery.fetchNextPage()} type="button">{customersQuery.isFetchingNextPage ? "Đang tải..." : "Tải thêm khách hàng"}</button> : null}
          </label>
          <label className="block text-sm font-semibold text-slate-700">Công việc
            <select className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" required value={draft.jobId} onChange={(event) => setDraft({ ...draft, jobId: event.target.value })}>
              <option value="">Chọn vị trí</option>{jobs.filter((item) => item.status === "active").map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
            {jobsQuery.hasNextPage ? <button className="mt-2 text-xs font-semibold text-blue-700" disabled={jobsQuery.isFetchingNextPage} onClick={() => void jobsQuery.fetchNextPage()} type="button">{jobsQuery.isFetchingNextPage ? "Đang tải..." : "Tải thêm công việc"}</button> : null}
          </label>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="block text-sm font-semibold text-slate-700">Bắt đầu<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" required type="datetime-local" value={draft.startsAt} onChange={(event) => setDraft({ ...draft, startsAt: event.target.value })} /></label>
            <label className="block text-sm font-semibold text-slate-700">Kết thúc<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" required type="datetime-local" value={draft.endsAt} onChange={(event) => setDraft({ ...draft, endsAt: event.target.value })} /></label>
          </div>
          <label className="block text-sm font-semibold text-slate-700">Số nhân viên<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" min="1" required type="number" value={draft.requiredWorkers} onChange={(event) => setDraft({ ...draft, requiredWorkers: event.target.value })} /></label>
          <label className="block text-sm font-semibold text-slate-700">Ghi chú<textarea className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" rows={3} value={draft.notes} onChange={(event) => setDraft({ ...draft, notes: event.target.value })} /></label>
          <button className="action-primary w-full" disabled={createMutation.isPending} type="submit">{createMutation.isPending ? <RefreshCw className="size-4 animate-spin" /> : <CalendarPlus2 className="size-4" />}Tạo ca</button>
        </form>
      </section> : null}

      <div className="space-y-5">
        {message ? <div className="rounded-xl border border-blue-200 bg-blue-50 px-4 py-3 text-sm font-medium text-blue-800">{message}</div> : null}
        <section className="panel overflow-hidden">
          <div className="border-b border-slate-200 px-5 py-4"><h2 className="font-bold text-slate-950">{canManage ? "Chọn ca để phân công" : "Danh sách ca đã điều phối"}</h2><p className="mt-1 text-sm text-slate-500">{canManage ? "Hệ thống tự loại nhân viên sai vị trí hoặc trùng lịch." : "Chế độ chỉ xem; chủ doanh nghiệp chịu trách nhiệm tạo ca và phân công."}</p></div>
          {shiftsQuery.error ? <p className="p-5 text-sm text-red-600"><CircleAlert className="mr-2 inline size-4" />{friendlyApiError(shiftsQuery.error, "Không tải được danh sách ca.")}</p> : (
          <div className="max-h-72 divide-y divide-slate-100 overflow-y-auto">{shifts.map((shift: ScopedStaffingShift): React.JSX.Element => <button className={`grid w-full gap-1 px-5 py-3 text-left hover:bg-slate-50 ${selectedShiftId === shift.id ? "bg-blue-50" : ""}`} key={shift.id} onClick={(): void => setSelectedShiftId(shift.id)} type="button"><span className="font-bold text-slate-900">{customerNames.get(shift.customer_id) ?? "Khách hàng"} · {formatDateTime(shift.starts_at)}</span><span className="text-xs text-slate-500">{scope.branches.find((branch): boolean => branch.id === shift.branch_id)?.name ?? "Chi nhánh"} · Cần {shift.required_workers} người · {shiftStatusLabel(shift.status)}</span></button>)}</div>
          )}
          {shiftsQuery.hasNextPage ? <div className="border-t border-slate-100 p-3 text-center"><button className="action-secondary min-h-9 px-4" disabled={shiftsQuery.isFetchingNextPage} onClick={() => void shiftsQuery.fetchNextPage()} type="button">{shiftsQuery.isFetchingNextPage ? "Đang tải..." : "Tải thêm ca"}</button></div> : null}
        </section>

        {selectedShiftId ? <section className="panel overflow-hidden">
          <div className="flex items-center gap-3 border-b border-slate-200 px-5 py-4"><UsersRound className="size-5 text-blue-600" /><div><h2 className="font-bold text-slate-950">Nhân viên có thể phân công</h2><p className="text-sm text-slate-500">Phù hợp dựa trên vị trí chính có hiệu lực tại ngày làm.</p></div></div>
          <div className="divide-y divide-slate-100">{candidates.map((candidate: StaffingCandidate): React.JSX.Element => {
            const eligible: boolean = candidate.suitable && candidate.available && !candidate.already_assigned;
            return <div className="flex items-center justify-between gap-3 px-5 py-4" key={candidate.employee_id}><div><p className="font-bold text-slate-900">{candidate.display_name}</p><p className="text-xs text-slate-500">{candidate.employee_code} · {!candidate.suitable ? "Không đúng vị trí" : !candidate.available ? "Trùng lịch" : candidate.already_assigned ? "Đã phân công" : "Sẵn sàng"}</p></div>{canManage ? <button className="action-secondary min-h-9 px-3" disabled={!eligible || assignMutation.isPending || selectedShift === null} onClick={(): void => assignMutation.mutate({ branchId: selectedShift?.branch_id ?? "", shiftId: selectedShiftId, employeeId: candidate.employee_id })} type="button"><UserPlus className="size-4" />Phân công</button> : null}</div>;
          })}{candidates.length === 0 ? <p className="p-6 text-center text-sm text-slate-500">Chưa có nhân viên hoạt động.</p> : null}</div>
          {candidatesQuery.hasNextPage ? <div className="border-t border-slate-100 p-3 text-center"><button className="action-secondary min-h-9 px-4" disabled={candidatesQuery.isFetchingNextPage} onClick={() => void candidatesQuery.fetchNextPage()} type="button">{candidatesQuery.isFetchingNextPage ? "Đang tải..." : "Tải thêm nhân viên"}</button></div> : null}
        </section> : null}
      </div>
    </div>
  );
}
