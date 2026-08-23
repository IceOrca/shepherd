import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  CheckCircle2,
  CircleAlert,
  Clock3,
  LogIn,
  LogOut,
  MapPin,
  RefreshCw,
  ShieldCheck,
  UserRoundCheck,
  UsersRound,
} from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import type {
  PermissionCode,
  UrgentWorkCustomer,
  UrgentWorkEmployee,
  UrgentWorkItem,
} from "../../api/generated/contracts";
import { friendlyApiError, isRetryableApiError } from "../../shared/api/client";
import { formatDateTime, formatDuration } from "../../shared/lib/format";
import { useOnlineStatus } from "../../shared/lib/useOnlineStatus";
import { useAuth } from "../auth/AuthProvider";
import {
  endUrgentWork,
  listOwnUrgentWork,
  listTeamUrgentWork,
  listUrgentEmployees,
  listUrgentCustomers,
  operationsQueryKeys,
  startUrgentWork,
  type UrgentEndActionInput,
  type UrgentStartActionInput,
} from "./api";

interface Feedback {
  kind: "success" | "error";
  message: string;
}

function sourceLabel(source: UrgentWorkItem["start_source"] | UrgentWorkItem["end_source"]): string {
  return source === "peer" ? "Đồng nghiệp ghi hộ" : "Tự ghi nhận";
}

function employeeSelectionDescription(employee: UrgentWorkEmployee): string {
  if (employee.has_open_work) {
    return "Đang có phiên làm việc mở";
  }
  return employee.is_self ? "Tài khoản của bạn" : employee.employee_code;
}

export function UrgentWorkPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const queryClient: QueryClient = useQueryClient();
  const isOnline: boolean = useOnlineStatus();
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canRead: boolean = permissions.includes("business.urgent_work.read");
  const canStart: boolean = permissions.includes("business.urgent_work.start");
  const canManagePeers: boolean = permissions.includes("business.urgent_work.peer_manage");
  const [customerId, setCustomerId] = useState<string>("");
  const [selectedEmployeeIds, setSelectedEmployeeIds] = useState<string[]>([]);
  const [feedback, setFeedback] = useState<Feedback | null>(null);
  const [endingReportId, setEndingReportId] = useState<string | null>(null);

  const customersQuery: UseQueryResult<UrgentWorkCustomer[], Error> = useQuery({
    queryKey: operationsQueryKeys.urgentCustomers,
    queryFn: listUrgentCustomers,
    enabled: canRead,
  });
  const employeesQuery: UseQueryResult<UrgentWorkEmployee[], Error> = useQuery({
    queryKey: operationsQueryKeys.urgentEmployees,
    queryFn: listUrgentEmployees,
    enabled: canRead,
  });
  const ownWorkQuery: UseQueryResult<UrgentWorkItem[], Error> = useQuery({
    queryKey: operationsQueryKeys.urgentOwnWork,
    queryFn: listOwnUrgentWork,
    enabled: canRead,
  });
  const teamWorkQuery: UseQueryResult<UrgentWorkItem[], Error> = useQuery({
    queryKey: operationsQueryKeys.urgentTeamWork,
    queryFn: listTeamUrgentWork,
    enabled: canRead && canManagePeers,
  });

  const ownWork: UrgentWorkItem[] = useMemo<UrgentWorkItem[]>(
    (): UrgentWorkItem[] => ownWorkQuery.data ?? [],
    [ownWorkQuery.data],
  );
  const teamWork: UrgentWorkItem[] = useMemo<UrgentWorkItem[]>(
    (): UrgentWorkItem[] => teamWorkQuery.data ?? [],
    [teamWorkQuery.data],
  );
  const activeOwnWork: UrgentWorkItem | null =
    ownWork.find((work: UrgentWorkItem): boolean => work.status === "active") ?? null;
  const activeTeamWork: UrgentWorkItem[] = teamWork.filter(
    (work: UrgentWorkItem): boolean => work.status === "active",
  );
  const visibleActiveWork: UrgentWorkItem[] = activeOwnWork
    ? [
        activeOwnWork,
        ...activeTeamWork.filter((work: UrgentWorkItem): boolean => work.report_id !== activeOwnWork.report_id),
      ]
    : activeTeamWork;
  const selfEmployee: UrgentWorkEmployee | null =
    (employeesQuery.data ?? []).find((employee: UrgentWorkEmployee): boolean => employee.is_self) ?? null;
  const clockableEmployees: UrgentWorkEmployee[] = useMemo<UrgentWorkEmployee[]>(
    (): UrgentWorkEmployee[] => employeesQuery.data ?? [],
    [employeesQuery.data],
  );

  useEffect((): void => {
    if (activeOwnWork) {
      setCustomerId(activeOwnWork.claimed_customer_id);
      setSelectedEmployeeIds([]);
      return;
    }
    if (!customerId && (customersQuery.data?.length ?? 0) > 0) {
      const firstCustomerId: string | undefined = customersQuery.data?.at(0)?.customer_id;
      if (firstCustomerId) {
        setCustomerId(firstCustomerId);
      }
    }
  }, [activeOwnWork, customersQuery.data, customerId]);

  useEffect((): void => {
    if (!activeOwnWork && selfEmployee && !selfEmployee.has_open_work) {
      setSelectedEmployeeIds((current: string[]): string[] =>
        current.includes(selfEmployee.employee_id) ? current : [selfEmployee.employee_id, ...current],
      );
    }
  }, [activeOwnWork, selfEmployee]);

  useEffect((): void => {
    const clockableEmployeeIds: Set<string> = new Set<string>(
      clockableEmployees.map((employee: UrgentWorkEmployee): string => employee.employee_id),
    );
    setSelectedEmployeeIds((current: string[]): string[] => {
      const eligibleSelection: string[] = current.filter((employeeId: string): boolean =>
        clockableEmployeeIds.has(employeeId),
      );
      return eligibleSelection.length === current.length ? current : eligibleSelection;
    });
  }, [clockableEmployees]);

  const refreshUrgentWork = (): Promise<void> =>
    queryClient.invalidateQueries({ queryKey: ["operations", "urgent-work"] });

  const startMutation: UseMutationResult<UrgentWorkItem[], unknown, UrgentStartActionInput> = useMutation<
    UrgentWorkItem[],
    unknown,
    UrgentStartActionInput
  >({
    mutationFn: (input: UrgentStartActionInput): Promise<UrgentWorkItem[]> => startUrgentWork(input),
    retry: (failureCount: number, error: unknown): boolean => failureCount < 1 && isRetryableApiError(error),
    onSuccess: (work: UrgentWorkItem[]): void => {
      setFeedback({
        kind: "success",
        message: `Đã bắt đầu cho ${work.length} nhân viên. Thời gian được ghi nhận theo máy chủ.`,
      });
      setSelectedEmployeeIds([]);
      void refreshUrgentWork();
    },
    onError: (error: unknown): void => {
      setFeedback({ kind: "error", message: friendlyApiError(error, "Không thể bắt đầu công việc khẩn.") });
    },
  });

  const endMutation: UseMutationResult<UrgentWorkItem, unknown, UrgentEndActionInput> = useMutation<
    UrgentWorkItem,
    unknown,
    UrgentEndActionInput
  >({
    mutationFn: (input: UrgentEndActionInput): Promise<UrgentWorkItem> => endUrgentWork(input),
    retry: (failureCount: number, error: unknown): boolean => failureCount < 1 && isRetryableApiError(error),
    onSuccess: (work: UrgentWorkItem): void => {
      setFeedback({
        kind: "success",
        message: `Đã kết thúc công việc của ${work.employee_name}. Thời gian được ghi nhận theo máy chủ.`,
      });
      void refreshUrgentWork();
    },
    onError: (error: unknown): void => {
      setFeedback({ kind: "error", message: friendlyApiError(error, "Không thể kết thúc công việc.") });
    },
    onSettled: (): void => setEndingReportId(null),
  });

  const toggleEmployee = (employee: UrgentWorkEmployee): void => {
    if (employee.has_open_work || (!canManagePeers && !employee.is_self)) {
      return;
    }
    if (!activeOwnWork && employee.is_self) {
      return;
    }
    setSelectedEmployeeIds((current: string[]): string[] =>
      current.includes(employee.employee_id)
        ? current.filter((employeeId: string): boolean => employeeId !== employee.employee_id)
        : [...current, employee.employee_id],
    );
  };

  const submitStart = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    if (!isOnline) {
      setFeedback({ kind: "error", message: "Thiết bị đang ngoại tuyến. Hãy kết nối mạng trước khi ghi nhận." });
      return;
    }
    if (!customerId || selectedEmployeeIds.length === 0) {
      setFeedback({ kind: "error", message: "Hãy chọn nơi làm việc và ít nhất một nhân viên." });
      return;
    }
    setFeedback(null);
    startMutation.mutate({
      idempotencyKey: crypto.randomUUID(),
      payload: {
        customer_id: customerId,
        employee_ids: selectedEmployeeIds,
        latitude: null,
        longitude: null,
        accuracy_meters: null,
      },
    });
  };

  const finishWork = (work: UrgentWorkItem): void => {
    if (!isOnline) {
      setFeedback({ kind: "error", message: "Thiết bị đang ngoại tuyến. Hãy kết nối mạng trước khi ghi nhận." });
      return;
    }
    setFeedback(null);
    setEndingReportId(work.report_id);
    endMutation.mutate({
      idempotencyKey: crypto.randomUUID(),
      reportId: work.report_id,
      payload: { latitude: null, longitude: null, accuracy_meters: null },
    });
  };

  if (!canRead) {
    return (
      <section className="panel p-8 text-center">
        <ShieldCheck className="mx-auto size-10 text-slate-400" />
        <h2 className="mt-4 text-lg font-bold text-slate-950">Chưa có quyền ghi nhận công việc</h2>
        <p className="mt-2 text-sm text-slate-500">Vui lòng liên hệ người quản lý để được cấp quyền phù hợp.</p>
      </section>
    );
  }

  const firstError: unknown =
    customersQuery.error ??
    employeesQuery.error ??
    ownWorkQuery.error ??
    (canManagePeers ? teamWorkQuery.error : null);
  const isPending: boolean =
    customersQuery.isPending ||
    employeesQuery.isPending ||
    ownWorkQuery.isPending ||
    (canManagePeers && teamWorkQuery.isPending);

  if (isPending) {
    return (
      <section className="panel p-8 text-center text-sm font-medium text-slate-500">
        <RefreshCw className="mr-2 inline size-4 animate-spin" />
        Đang tải nơi làm việc và nhân viên...
      </section>
    );
  }

  if (firstError) {
    return (
      <section className="panel p-8 text-center">
        <CircleAlert className="mx-auto size-10 text-red-500" />
        <p className="mt-3 text-sm text-red-700">{friendlyApiError(firstError, "Không thể tải dữ liệu ghi nhận.")}</p>
      </section>
    );
  }

  return (
    <div className="space-y-5">
      <section className="overflow-hidden rounded-2xl bg-gradient-to-br from-blue-700 via-blue-600 to-cyan-500 p-6 text-white shadow-xl shadow-blue-900/10">
        <p className="text-sm font-bold text-blue-100">Ghi nhận nhanh tại hiện trường</p>
        <h2 className="mt-2 text-2xl font-black">Không cần tạo ca trước</h2>
        <p className="mt-2 max-w-2xl text-sm leading-6 text-blue-50">
          Chọn đúng khách hàng là nơi làm việc, chọn bạn và đồng nghiệp đang có mặt, rồi bấm Bắt đầu. Hệ thống dùng thời gian máy chủ; GPS hiện đang tắt.
        </p>
      </section>

      {feedback ? (
        <div
          className={`flex items-start gap-3 rounded-2xl border px-4 py-3 text-sm font-medium ${
            feedback.kind === "success"
              ? "border-emerald-200 bg-emerald-50 text-emerald-800"
              : "border-red-200 bg-red-50 text-red-800"
          }`}
        >
          {feedback.kind === "success" ? (
            <CheckCircle2 className="mt-0.5 size-5 shrink-0" />
          ) : (
            <CircleAlert className="mt-0.5 size-5 shrink-0" />
          )}
          <span>{feedback.message}</span>
        </div>
      ) : null}

      {visibleActiveWork.length > 0 ? (
        <section className="panel overflow-hidden">
          <div className="border-b border-slate-200 px-5 py-4">
            <h2 className="font-bold text-slate-950">Đang làm việc tại khách hàng</h2>
            <p className="mt-1 text-sm text-slate-500">Bạn có thể kết thúc cho chính mình hoặc đồng nghiệp cùng nơi làm việc.</p>
          </div>
          <div className="divide-y divide-slate-100">
            {visibleActiveWork.map((work: UrgentWorkItem): React.JSX.Element => {
              const isSelf: boolean = work.employee_id === selfEmployee?.employee_id;
              const canFinish: boolean = canStart && (isSelf || canManagePeers);
              const isEnding: boolean = endingReportId === work.report_id && endMutation.isPending;
              return (
                <article className="grid gap-4 px-5 py-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center" key={work.report_id}>
                  <div>
                    <div className="flex flex-wrap items-center gap-2">
                      <p className="font-bold text-slate-950">{work.employee_name}</p>
                      <span className="rounded-full bg-emerald-50 px-2.5 py-1 text-[11px] font-bold text-emerald-700">Đang làm</span>
                    </div>
                    <p className="mt-1 flex items-center gap-1.5 text-sm text-slate-500">
                      <MapPin className="size-4 text-blue-600" />
                      {work.customer_name}
                    </p>
                    <p className="mt-1 text-xs text-slate-500">
                      Bắt đầu {formatDateTime(work.started_at)} · {sourceLabel(work.start_source)}
                    </p>
                  </div>
                  <button
                    className="action-secondary"
                    disabled={!canFinish || !isOnline || endMutation.isPending}
                    onClick={(): void => finishWork(work)}
                    type="button"
                  >
                    {isEnding ? <RefreshCw className="size-4 animate-spin" /> : <LogOut className="size-4" />}
                    {isEnding ? "Đang kết thúc..." : `Kết thúc ${isSelf ? "của tôi" : "ghi hộ"}`}
                  </button>
                </article>
              );
            })}
          </div>
        </section>
      ) : null}

      <form className="panel p-5 sm:p-6" onSubmit={submitStart}>
        <div className="flex items-start gap-3">
          <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-blue-50 text-blue-700">
            <LogIn className="size-5" />
          </div>
          <div>
            <h2 className="font-bold text-slate-950">Bắt đầu công việc</h2>
            <p className="mt-1 text-sm text-slate-500">
              {activeOwnWork
                ? "Bạn đang làm tại khách hàng này; có thể bổ sung đồng nghiệp vừa đến."
                : "Lần ghi đầu tiên phải bao gồm chính bạn."}
            </p>
          </div>
        </div>

        <label className="mt-5 block text-sm font-semibold text-slate-700">
          Khách hàng / nơi làm việc
          <select
            className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
            disabled={Boolean(activeOwnWork) || startMutation.isPending}
            onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setCustomerId(event.target.value)}
            required
            value={customerId}
          >
            <option value="">Chọn khách hàng</option>
            {(customersQuery.data ?? []).map((customer: UrgentWorkCustomer): React.JSX.Element => (
              <option key={customer.customer_id} value={customer.customer_id}>
                {customer.customer_name}
              </option>
            ))}
          </select>
        </label>

        <div className="mt-5">
          <div className="flex items-center justify-between gap-3">
            <p className="text-sm font-semibold text-slate-700">Nhân viên được phép ghi nhận công việc</p>
            <span className="text-xs font-semibold text-slate-500">Đã chọn {selectedEmployeeIds.length}</span>
          </div>
          <div className="mt-2 grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
            {clockableEmployees.map((employee: UrgentWorkEmployee): React.JSX.Element => {
              const selected: boolean = selectedEmployeeIds.includes(employee.employee_id);
              const lockedSelf: boolean = !activeOwnWork && employee.is_self;
              const disabled: boolean = employee.has_open_work || (!canManagePeers && !employee.is_self);
              return (
                <button
                  className={`flex min-h-14 items-center gap-3 rounded-xl border px-3 py-2 text-left transition ${
                    selected
                      ? "border-blue-500 bg-blue-50 text-blue-950"
                      : disabled
                        ? "border-slate-100 bg-slate-50 text-slate-400"
                        : "border-slate-200 bg-white text-slate-800 hover:border-blue-300"
                  }`}
                  disabled={disabled || lockedSelf || startMutation.isPending}
                  key={employee.employee_id}
                  onClick={(): void => toggleEmployee(employee)}
                  type="button"
                >
                  {employee.is_self ? <UserRoundCheck className="size-5 shrink-0" /> : <UsersRound className="size-5 shrink-0" />}
                  <span className="min-w-0">
                    <span className="block truncate text-sm font-bold">{employee.display_name}</span>
                    <span className="block truncate text-xs">{employeeSelectionDescription(employee)}</span>
                  </span>
                  <span className={`ml-auto size-4 rounded-full border ${selected ? "border-blue-600 bg-blue-600 ring-2 ring-blue-200" : "border-slate-300"}`} />
                </button>
              );
            })}
            {!employeesQuery.isLoading && clockableEmployees.length === 0 ? (
              <p className="text-sm text-slate-500">Không có nhân viên thuộc nhóm làm việc để ghi nhận.</p>
            ) : null}
          </div>
        </div>

        <button
          className="action-primary mt-5 w-full sm:w-auto"
          disabled={!canStart || !isOnline || !customerId || selectedEmployeeIds.length === 0 || startMutation.isPending}
          type="submit"
        >
          {startMutation.isPending ? <RefreshCw className="size-4 animate-spin" /> : <LogIn className="size-4" />}
          {startMutation.isPending ? "Đang ghi nhận..." : `Bắt đầu cho ${selectedEmployeeIds.length} người`}
        </button>
      </form>

      {ownWork.length > 0 ? (
        <section className="panel overflow-hidden">
          <div className="border-b border-slate-200 px-5 py-4">
            <h2 className="font-bold text-slate-950">Lịch sử của tôi</h2>
          </div>
          <div className="divide-y divide-slate-100">
            {ownWork.slice(0, 20).map((work: UrgentWorkItem): React.JSX.Element => (
              <article className="grid gap-2 px-5 py-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center" key={work.report_id}>
                <div>
                  <p className="font-semibold text-slate-900">{work.customer_name}</p>
                  <p className="mt-1 text-xs text-slate-500">
                    {formatDateTime(work.started_at)} · {sourceLabel(work.start_source)}
                  </p>
                </div>
                <p className="flex items-center gap-2 text-sm font-bold text-slate-700">
                  <Clock3 className="size-4 text-violet-600" />
                  {work.worked_seconds === null ? "Đang làm" : formatDuration(work.worked_seconds)}
                </p>
              </article>
            ))}
          </div>
        </section>
      ) : null}
    </div>
  );
}
