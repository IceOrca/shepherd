import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CalendarPlus2, CircleAlert, RefreshCw, UserPlus, UsersRound } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import type { StaffingShiftCreateRequest } from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { formatDateTime, shiftStatusLabel } from "../../shared/lib/format";
import { useAuth } from "../auth/AuthProvider";
import {
  createShiftAssignment,
  createStaffingShift,
  listCustomerFacilities,
  listCustomers,
  listJobs,
  listShiftCandidates,
  listStaffingShifts,
  operationsQueryKeys,
} from "./api";

interface ShiftDraft {
  customerId: string;
  facilityId: string;
  jobId: string;
  startsAt: string;
  endsAt: string;
  requiredWorkers: string;
  notes: string;
}

const emptyDraft: ShiftDraft = {
  customerId: "",
  facilityId: "",
  jobId: "",
  startsAt: "",
  endsAt: "",
  requiredWorkers: "1",
  notes: "",
};

export function ShiftCoordinationPage() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const permissions = auth.profile?.permissions ?? [];
  const canManage = permissions.includes("business.shifts.manage");
  const [draft, setDraft] = useState<ShiftDraft>(emptyDraft);
  const [selectedShiftId, setSelectedShiftId] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const customersQuery = useQuery({ queryKey: operationsQueryKeys.customers, queryFn: listCustomers });
  const jobsQuery = useQuery({ queryKey: operationsQueryKeys.jobs, queryFn: listJobs });
  const shiftsQuery = useQuery({ queryKey: operationsQueryKeys.shifts, queryFn: listStaffingShifts });
  const facilitiesQuery = useQuery({
    queryKey: operationsQueryKeys.facilities(draft.customerId),
    queryFn: () => listCustomerFacilities(draft.customerId),
    enabled: Boolean(draft.customerId),
  });
  const candidatesQuery = useQuery({
    queryKey: operationsQueryKeys.candidates(selectedShiftId ?? ""),
    queryFn: () => listShiftCandidates(selectedShiftId ?? ""),
    enabled: Boolean(selectedShiftId),
  });

  const customerNames = useMemo(
    () => new Map((customersQuery.data ?? []).map((customer) => [customer.id, customer.name])),
    [customersQuery.data],
  );
  const shifts = useMemo(
    () => [...(shiftsQuery.data ?? [])].sort((a, b) => new Date(a.starts_at).getTime() - new Date(b.starts_at).getTime()),
    [shiftsQuery.data],
  );

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
    mutationFn: ({ shiftId, employeeId }: { shiftId: string; employeeId: string }) =>
      createShiftAssignment(shiftId, { employee_id: employeeId }),
    onSuccess: () => {
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
      customer_facility_id: draft.facilityId,
      job_id: draft.jobId,
      starts_at: new Date(draft.startsAt).toISOString(),
      ends_at: new Date(draft.endsAt).toISOString(),
      required_workers: Number(draft.requiredWorkers),
      notes: draft.notes.trim() || null,
    };
    createMutation.mutate(payload);
  };

  if (!canManage) {
    return <section className="panel p-8 text-center text-sm text-slate-500">Bạn chưa có quyền điều phối ca.</section>;
  }

  return (
    <div className="grid gap-5 xl:grid-cols-[minmax(320px,0.8fr)_minmax(0,1.2fr)]">
      <section className="panel p-5 sm:p-6">
        <div className="flex items-center gap-3">
          <div className="grid size-10 place-items-center rounded-xl bg-blue-50 text-blue-700"><CalendarPlus2 className="size-5" /></div>
          <div><h2 className="font-bold text-slate-950">Tạo ca theo yêu cầu khách hàng</h2><p className="text-sm text-slate-500">Địa điểm và công việc được khóa theo ca.</p></div>
        </div>
        <form className="mt-5 space-y-4" onSubmit={submit}>
          <label className="block text-sm font-semibold text-slate-700">Khách hàng
            <select className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" required value={draft.customerId} onChange={(event) => setDraft({ ...draft, customerId: event.target.value, facilityId: "" })}>
              <option value="">Chọn khách hàng</option>{customersQuery.data?.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
          </label>
          <label className="block text-sm font-semibold text-slate-700">Cơ sở làm việc
            <select className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" required value={draft.facilityId} onChange={(event) => setDraft({ ...draft, facilityId: event.target.value })}>
              <option value="">Chọn cơ sở</option>{facilitiesQuery.data?.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
          </label>
          <label className="block text-sm font-semibold text-slate-700">Công việc
            <select className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" required value={draft.jobId} onChange={(event) => setDraft({ ...draft, jobId: event.target.value })}>
              <option value="">Chọn vị trí</option>{jobsQuery.data?.filter((item) => item.status === "active").map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
          </label>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="block text-sm font-semibold text-slate-700">Bắt đầu<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" required type="datetime-local" value={draft.startsAt} onChange={(event) => setDraft({ ...draft, startsAt: event.target.value })} /></label>
            <label className="block text-sm font-semibold text-slate-700">Kết thúc<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" required type="datetime-local" value={draft.endsAt} onChange={(event) => setDraft({ ...draft, endsAt: event.target.value })} /></label>
          </div>
          <label className="block text-sm font-semibold text-slate-700">Số nhân viên<input className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" min="1" required type="number" value={draft.requiredWorkers} onChange={(event) => setDraft({ ...draft, requiredWorkers: event.target.value })} /></label>
          <label className="block text-sm font-semibold text-slate-700">Ghi chú<textarea className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" rows={3} value={draft.notes} onChange={(event) => setDraft({ ...draft, notes: event.target.value })} /></label>
          <button className="action-primary w-full" disabled={createMutation.isPending} type="submit">{createMutation.isPending ? <RefreshCw className="size-4 animate-spin" /> : <CalendarPlus2 className="size-4" />}Tạo ca</button>
        </form>
      </section>

      <div className="space-y-5">
        {message ? <div className="rounded-xl border border-blue-200 bg-blue-50 px-4 py-3 text-sm font-medium text-blue-800">{message}</div> : null}
        <section className="panel overflow-hidden">
          <div className="border-b border-slate-200 px-5 py-4"><h2 className="font-bold text-slate-950">Chọn ca để phân công</h2><p className="mt-1 text-sm text-slate-500">Hệ thống tự loại nhân viên sai vị trí hoặc trùng lịch.</p></div>
          {shiftsQuery.error ? <p className="p-5 text-sm text-red-600"><CircleAlert className="mr-2 inline size-4" />{friendlyApiError(shiftsQuery.error, "Không tải được danh sách ca.")}</p> : (
            <div className="max-h-72 divide-y divide-slate-100 overflow-y-auto">{shifts.map((shift) => <button className={`grid w-full gap-1 px-5 py-3 text-left hover:bg-slate-50 ${selectedShiftId === shift.id ? "bg-blue-50" : ""}`} key={shift.id} onClick={() => setSelectedShiftId(shift.id)} type="button"><span className="font-bold text-slate-900">{customerNames.get(shift.customer_id) ?? "Khách hàng"} · {formatDateTime(shift.starts_at)}</span><span className="text-xs text-slate-500">Cần {shift.required_workers} người · {shiftStatusLabel(shift.status)}</span></button>)}</div>
          )}
        </section>

        {selectedShiftId ? <section className="panel overflow-hidden">
          <div className="flex items-center gap-3 border-b border-slate-200 px-5 py-4"><UsersRound className="size-5 text-blue-600" /><div><h2 className="font-bold text-slate-950">Nhân viên có thể phân công</h2><p className="text-sm text-slate-500">Phù hợp dựa trên vị trí chính có hiệu lực tại ngày làm.</p></div></div>
          <div className="divide-y divide-slate-100">{candidatesQuery.data?.map((candidate) => {
            const eligible = candidate.suitable && candidate.available && !candidate.already_assigned;
            return <div className="flex items-center justify-between gap-3 px-5 py-4" key={candidate.employee_id}><div><p className="font-bold text-slate-900">{candidate.display_name}</p><p className="text-xs text-slate-500">{candidate.employee_code} · {!candidate.suitable ? "Không đúng vị trí" : !candidate.available ? "Trùng lịch" : candidate.already_assigned ? "Đã phân công" : "Sẵn sàng"}</p></div><button className="action-secondary min-h-9 px-3" disabled={!eligible || assignMutation.isPending} onClick={() => assignMutation.mutate({ shiftId: selectedShiftId, employeeId: candidate.employee_id })} type="button"><UserPlus className="size-4" />Phân công</button></div>;
          })}{candidatesQuery.data?.length === 0 ? <p className="p-6 text-center text-sm text-slate-500">Chưa có nhân viên hoạt động.</p> : null}</div>
        </section> : null}
      </div>
    </div>
  );
}
