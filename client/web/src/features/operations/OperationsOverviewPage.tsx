import { useQuery } from "@tanstack/react-query";
import {
  BriefcaseBusiness,
  Building2,
  CalendarCheck2,
  CircleAlert,
  Clock3,
  RefreshCw,
  UsersRound,
} from "lucide-react";
import { useMemo } from "react";
import { Link } from "react-router-dom";
import type { StaffingShift } from "../../api/generated/contracts";
import { useAuth } from "../auth/AuthProvider";
import { friendlyApiError } from "../../shared/api/client";
import { formatDateTime, formatDuration, shiftStatusLabel } from "../../shared/lib/format";
import {
  listCustomers,
  listOwnAssignments,
  listStaffingShifts,
  operationsQueryKeys,
} from "./api";

function MetricCard({
  icon: Icon,
  label,
  value,
  note,
  tone,
}: {
  icon: typeof BriefcaseBusiness;
  label: string;
  value: string | number;
  note: string;
  tone: "blue" | "emerald" | "amber" | "violet";
}) {
  const tones = {
    blue: "bg-blue-50 text-blue-700",
    emerald: "bg-emerald-50 text-emerald-700",
    amber: "bg-amber-50 text-amber-700",
    violet: "bg-violet-50 text-violet-700",
  };

  return (
    <article className="panel p-5">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-sm font-medium text-slate-500">{label}</p>
          <p className="mt-2 text-3xl font-black tracking-tight text-slate-950">{value}</p>
          <p className="mt-1 text-xs text-slate-500">{note}</p>
        </div>
        <div className={`grid size-11 shrink-0 place-items-center rounded-2xl ${tones[tone]}`}>
          <Icon className="size-5" />
        </div>
      </div>
    </article>
  );
}

function OverviewSkeleton() {
  return (
    <div className="space-y-6" aria-label="Đang tải tổng quan">
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {Array.from({ length: 4 }, (_, index) => (
          <div className="panel p-5" key={index}>
            <div className="skeleton h-4 w-28" />
            <div className="skeleton mt-4 h-9 w-16" />
            <div className="skeleton mt-3 h-3 w-36" />
          </div>
        ))}
      </div>
      <div className="panel p-6">
        <div className="skeleton h-6 w-48" />
        <div className="mt-5 space-y-3">
          <div className="skeleton h-16 w-full" />
          <div className="skeleton h-16 w-full" />
          <div className="skeleton h-16 w-full" />
        </div>
      </div>
    </div>
  );
}

function statusClass(status: StaffingShift["status"]): string {
  switch (status) {
    case "open":
      return "bg-amber-50 text-amber-700";
    case "filled":
      return "bg-blue-50 text-blue-700";
    case "in_progress":
      return "bg-emerald-50 text-emerald-700";
    case "completed":
      return "bg-slate-100 text-slate-600";
    case "cancelled":
      return "bg-red-50 text-red-700";
  }
}

export function OperationsOverviewPage() {
  const auth = useAuth();
  const permissions = auth.profile?.permissions ?? [];
  const canReadShifts = permissions.includes("business.shifts.read");
  const canReadCustomers = permissions.includes("business.customers.read");
  const canReadOwnAssignments = permissions.includes("business.staffing_work.self.read");

  const shiftsQuery = useQuery({
    queryKey: operationsQueryKeys.shifts,
    queryFn: listStaffingShifts,
    enabled: canReadShifts,
  });
  const customersQuery = useQuery({
    queryKey: operationsQueryKeys.customers,
    queryFn: listCustomers,
    enabled: canReadCustomers,
  });
  const ownAssignmentsQuery = useQuery({
    queryKey: operationsQueryKeys.ownAssignments,
    queryFn: listOwnAssignments,
    enabled: canReadOwnAssignments && !canReadShifts,
  });

  const relevantQueries = canReadShifts
    ? [shiftsQuery, ...(canReadCustomers ? [customersQuery] : [])]
    : canReadOwnAssignments
      ? [ownAssignmentsQuery]
      : [];
  const isPending = relevantQueries.some((query) => query.isPending);
  const firstError = relevantQueries.find((query) => query.error)?.error;

  const upcomingShifts = useMemo(
    () =>
      [...(shiftsQuery.data ?? [])]
        .filter((shift) => shift.status !== "cancelled" && new Date(shift.ends_at).getTime() >= Date.now())
        .sort((left, right) => new Date(left.starts_at).getTime() - new Date(right.starts_at).getTime())
        .slice(0, 6),
    [shiftsQuery.data],
  );
  const customerNames = useMemo(
    () => new Map((customersQuery.data ?? []).map((customer) => [customer.id, customer.name])),
    [customersQuery.data],
  );

  if (isPending) {
    return <OverviewSkeleton />;
  }

  if (firstError) {
    return (
      <section className="panel p-8 text-center">
        <CircleAlert className="mx-auto size-10 text-red-500" />
        <h2 className="mt-4 text-lg font-bold text-slate-950">Chưa thể tải dữ liệu vận hành</h2>
        <p className="mx-auto mt-2 max-w-lg text-sm leading-6 text-slate-500">
          {friendlyApiError(firstError, "Máy chủ chưa thể trả về dữ liệu tổng quan.")}
        </p>
        <button
          className="action-secondary mt-5"
          onClick={() => void Promise.all(relevantQueries.map((query) => query.refetch()))}
          type="button"
        >
          <RefreshCw className="size-4" />
          Thử tải lại
        </button>
      </section>
    );
  }

  if (!canReadShifts) {
    const assignments = ownAssignmentsQuery.data ?? [];
    const active = assignments.filter((assignment) => assignment.is_working);
    const scheduled = assignments.filter(
      (assignment) => assignment.status === "assigned" && new Date(assignment.ends_at).getTime() >= Date.now(),
    );
    const workedSeconds = assignments.reduce((sum, assignment) => sum + assignment.observed_worked_seconds, 0);

    return (
      <div className="space-y-6">
        <section className="overflow-hidden rounded-2xl bg-gradient-to-br from-blue-700 via-blue-600 to-cyan-500 p-6 text-white shadow-xl shadow-blue-900/10 sm:p-8">
          <p className="text-sm font-bold text-blue-100">Chào {auth.profile?.username},</p>
          <h2 className="mt-2 max-w-2xl text-2xl font-black tracking-tight sm:text-3xl">
            Sẵn sàng cho ca làm tiếp theo của bạn?
          </h2>
          <p className="mt-3 max-w-xl text-sm leading-6 text-blue-50">
            Mở danh sách ca để xem nơi làm việc và ghi nhận bắt đầu hoặc kết thúc ngay tại hiện trường.
          </p>
          <Link className="mt-6 inline-flex min-h-11 items-center rounded-xl bg-white px-5 text-sm font-bold text-blue-700 shadow-lg" to="/van-hanh/ca-lam-cua-toi">
            Mở ca làm của tôi
          </Link>
        </section>

        <div className="grid gap-4 sm:grid-cols-3">
          <MetricCard icon={Clock3} label="Đang làm việc" value={active.length} note="Ca đang mở phiên làm" tone="emerald" />
          <MetricCard icon={CalendarCheck2} label="Ca sắp tới" value={scheduled.length} note="Ca còn hiệu lực" tone="blue" />
          <MetricCard icon={BriefcaseBusiness} label="Đã ghi nhận" value={formatDuration(workedSeconds)} note="Tổng thời gian quan sát" tone="violet" />
        </div>
      </div>
    );
  }

  const shifts = shiftsQuery.data ?? [];
  const openCount = shifts.filter((shift) => shift.status === "open").length;
  const inProgressCount = shifts.filter((shift) => shift.status === "in_progress").length;

  return (
    <div className="space-y-6">
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <MetricCard icon={BriefcaseBusiness} label="Tổng ca làm" value={shifts.length} note="Trong dữ liệu hiện có" tone="blue" />
        <MetricCard icon={UsersRound} label="Đang cần người" value={openCount} note="Cần điều phối nhân sự" tone="amber" />
        <MetricCard icon={Clock3} label="Đang diễn ra" value={inProgressCount} note="Ca đang hoạt động" tone="emerald" />
        <MetricCard icon={Building2} label="Khách hàng" value={customersQuery.data?.length ?? "—"} note="Đơn vị đang quản lý" tone="violet" />
      </div>

      <section className="panel overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-slate-200 px-5 py-4 sm:px-6">
          <div>
            <h2 className="font-bold text-slate-950">Ca đang và sắp diễn ra</h2>
            <p className="mt-1 text-sm text-slate-500">Ưu tiên theo thời gian bắt đầu gần nhất.</p>
          </div>
          <button
            className="action-secondary min-h-9 px-3"
            disabled={relevantQueries.some((query) => query.isFetching)}
            onClick={() => void Promise.all(relevantQueries.map((query) => query.refetch()))}
            type="button"
          >
            <RefreshCw className={`size-4 ${relevantQueries.some((query) => query.isFetching) ? "animate-spin" : ""}`} />
            Làm mới
          </button>
        </div>

        {upcomingShifts.length === 0 ? (
          <div className="px-6 py-12 text-center">
            <CalendarCheck2 className="mx-auto size-10 text-slate-300" />
            <p className="mt-3 font-semibold text-slate-700">Chưa có ca làm sắp tới</p>
            <p className="mt-1 text-sm text-slate-500">Các ca mới sẽ xuất hiện tại đây.</p>
          </div>
        ) : (
          <div className="divide-y divide-slate-100">
            {upcomingShifts.map((shift) => (
              <article className="grid gap-3 px-5 py-4 sm:grid-cols-[minmax(0,1fr)_auto_auto] sm:items-center sm:px-6" key={shift.id}>
                <div className="min-w-0">
                  <p className="truncate font-bold text-slate-900">
                    {customerNames.get(shift.customer_id) ?? `Khách hàng ${shift.customer_id.slice(0, 8)}`}
                  </p>
                  <p className="mt-1 text-sm text-slate-500">
                    {formatDateTime(shift.starts_at)} – {formatDateTime(shift.ends_at)}
                  </p>
                </div>
                <p className="text-sm font-semibold text-slate-600">Cần {shift.required_workers} người</p>
                <span className={`w-fit rounded-full px-3 py-1 text-xs font-bold ${statusClass(shift.status)}`}>
                  {shiftStatusLabel(shift.status)}
                </span>
              </article>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
