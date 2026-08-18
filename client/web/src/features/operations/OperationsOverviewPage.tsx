import { useQuery, type UseQueryResult } from "@tanstack/react-query";
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
import type { Customer, StaffingShift, UrgentWorkItem } from "../../api/generated/contracts";
import { useAuth } from "../auth/AuthProvider";
import { friendlyApiError } from "../../shared/api/client";
import { formatDateTime, formatDuration, shiftStatusLabel } from "../../shared/lib/format";
import {
  listCustomers,
  listOwnUrgentWork,
  listStaffingShifts,
  operationsQueryKeys,
} from "./api";

type MetricTone = "blue" | "emerald" | "amber" | "violet";

interface MetricCardProps {
  icon: typeof BriefcaseBusiness;
  label: string;
  value: string | number;
  note: string;
  tone: MetricTone;
}

interface QueryLifecycle {
  error: unknown;
  isFetching: boolean;
  isPending: boolean;
  refetch(): Promise<unknown>;
}

function MetricCard({
  icon: Icon,
  label,
  value,
  note,
  tone,
}: MetricCardProps): React.JSX.Element {
  const tones: Record<MetricTone, string> = {
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

function OverviewSkeleton(): React.JSX.Element {
  return (
    <div className="space-y-6" aria-label="Đang tải tổng quan">
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {Array.from({ length: 4 }, (_value: unknown, index: number): React.JSX.Element => (
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

export function OperationsOverviewPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const permissions: string[] = auth.profile?.permissions ?? [];
  const canReadShifts: boolean = permissions.includes("business.shifts.read");
  const canReadCustomers: boolean = permissions.includes("business.customers.read");
  const canReadUrgentWork: boolean = permissions.includes("business.urgent_work.read");

  const shiftsQuery: UseQueryResult<StaffingShift[], Error> = useQuery({
    queryKey: operationsQueryKeys.shifts,
    queryFn: listStaffingShifts,
    enabled: canReadShifts,
  });
  const customersQuery: UseQueryResult<Customer[], Error> = useQuery({
    queryKey: operationsQueryKeys.customers,
    queryFn: listCustomers,
    enabled: canReadCustomers,
  });
  const ownUrgentWorkQuery: UseQueryResult<UrgentWorkItem[], Error> = useQuery({
    queryKey: operationsQueryKeys.urgentOwnWork,
    queryFn: listOwnUrgentWork,
    enabled: canReadUrgentWork && !canReadShifts,
  });

  const relevantQueries: QueryLifecycle[] = canReadShifts
    ? [shiftsQuery, ...(canReadCustomers ? [customersQuery] : [])]
    : canReadUrgentWork
      ? [ownUrgentWorkQuery]
      : [];
  const isPending: boolean = relevantQueries.some((query: QueryLifecycle): boolean => query.isPending);
  const firstError: unknown = relevantQueries.find((query: QueryLifecycle): boolean => Boolean(query.error))?.error;

  const upcomingShifts: StaffingShift[] = useMemo<StaffingShift[]>(
    (): StaffingShift[] =>
      [...(shiftsQuery.data ?? [])]
        .filter(
          (shift: StaffingShift): boolean =>
            shift.status !== "cancelled" && new Date(shift.ends_at).getTime() >= Date.now(),
        )
        .sort(
          (left: StaffingShift, right: StaffingShift): number =>
            new Date(left.starts_at).getTime() - new Date(right.starts_at).getTime(),
        )
        .slice(0, 6),
    [shiftsQuery.data],
  );
  const customerNames: Map<string, string> = useMemo<Map<string, string>>(
    (): Map<string, string> =>
      new Map(
        (customersQuery.data ?? []).map((customer: Customer): [string, string] => [customer.id, customer.name]),
      ),
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
          onClick={(): void =>
            void Promise.all(relevantQueries.map((query: QueryLifecycle): Promise<unknown> => query.refetch()))
          }
          type="button"
        >
          <RefreshCw className="size-4" />
          Thử tải lại
        </button>
      </section>
    );
  }

  if (!canReadShifts) {
    const work: UrgentWorkItem[] = ownUrgentWorkQuery.data ?? [];
    const active: UrgentWorkItem[] = work.filter((item: UrgentWorkItem): boolean => item.status === "active");
    const completed: UrgentWorkItem[] = work.filter(
      (item: UrgentWorkItem): boolean => item.status === "completed" || item.status === "reconciled",
    );
    const workedSeconds: number = work.reduce(
      (sum: number, item: UrgentWorkItem): number => sum + (item.worked_seconds ?? 0),
      0,
    );

    return (
      <div className="space-y-6">
        <section className="overflow-hidden rounded-2xl bg-gradient-to-br from-blue-700 via-blue-600 to-cyan-500 p-6 text-white shadow-xl shadow-blue-900/10 sm:p-8">
          <p className="text-sm font-bold text-blue-100">Chào {auth.profile?.username},</p>
          <h2 className="mt-2 max-w-2xl text-2xl font-black tracking-tight sm:text-3xl">
            Ghi nhận công việc ngay khi đến cơ sở
          </h2>
          <p className="mt-3 max-w-xl text-sm leading-6 text-blue-50">
            Không cần chờ quản lý tạo ca. Chọn đúng cơ sở, chọn bạn và đồng nghiệp, rồi bắt đầu bằng thời gian máy chủ.
          </p>
          <Link className="mt-6 inline-flex min-h-11 items-center rounded-xl bg-white px-5 text-sm font-bold text-blue-700 shadow-lg" to="/operations/work">
            Ghi nhận công việc
          </Link>
        </section>

        <div className="grid gap-4 sm:grid-cols-3">
          <MetricCard icon={Clock3} label="Đang làm việc" value={active.length} note="Phiên công việc đang mở" tone="emerald" />
          <MetricCard icon={UsersRound} label="Đã hoàn thành" value={completed.length} note="Chờ hoặc đã đối soát" tone="blue" />
          <MetricCard icon={BriefcaseBusiness} label="Đã ghi nhận" value={formatDuration(workedSeconds)} note="Tổng thời gian quan sát" tone="violet" />
        </div>
      </div>
    );
  }

  const shifts: StaffingShift[] = shiftsQuery.data ?? [];
  const openCount: number = shifts.filter((shift: StaffingShift): boolean => shift.status === "open").length;
  const inProgressCount: number = shifts.filter(
    (shift: StaffingShift): boolean => shift.status === "in_progress",
  ).length;

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
            disabled={relevantQueries.some((query: QueryLifecycle): boolean => query.isFetching)}
            onClick={(): void =>
              void Promise.all(relevantQueries.map((query: QueryLifecycle): Promise<unknown> => query.refetch()))
            }
            type="button"
          >
            <RefreshCw
              className={`size-4 ${
                relevantQueries.some((query: QueryLifecycle): boolean => query.isFetching) ? "animate-spin" : ""
              }`}
            />
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
            {upcomingShifts.map((shift: StaffingShift): React.JSX.Element => (
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
