import {
  useInfiniteQuery,
  useMutation,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import {
  CheckCircle2,
  CircleAlert,
  ClipboardPlus,
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
  UrgentOwnWorkPageRsp,
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
  submitManualUrgentWork,
  type UrgentEndActionInput,
  type UrgentManualActionInput,
  type UrgentStartActionInput,
} from "./api";
import { ReconciliationPagination } from "./ReconciliationPagination";

interface Feedback {
  kind: "success" | "error";
  message: string;
}

function sourceLabel(source: UrgentWorkItem["start_source"] | UrgentWorkItem["end_source"]): string {
  if (source === null) {
    return "Chưa có nguồn kết thúc";
  }
  return source === "peer" ? "Đồng nghiệp ghi hộ" : "Tự ghi nhận";
}

function localDateTimeInput(date: Date): string {
  const offsetMilliseconds: number = date.getTimezoneOffset() * 60_000;
  return new Date(date.getTime() - offsetMilliseconds).toISOString().slice(0, 16);
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
  const [entryMode, setEntryMode] = useState<"live" | "manual">("live");
  const [manualCustomerId, setManualCustomerId] = useState<string>("");
  const [manualStartedAt, setManualStartedAt] = useState<string>(() =>
    localDateTimeInput(new Date(Date.now() - 8 * 60 * 60 * 1000)),
  );
  const [manualEndedAt, setManualEndedAt] = useState<string>(() => localDateTimeInput(new Date()));
  const [manualNote, setManualNote] = useState<string>("");
  const [historyPage, setHistoryPage] = useState<number>(1);

  const customersQuery = useInfiniteQuery({
    queryKey: operationsQueryKeys.urgentCustomers,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => listUrgentCustomers(pageParam),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    enabled: canRead,
  });
  const employeesQuery = useInfiniteQuery({
    queryKey: operationsQueryKeys.urgentEmployees,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => listUrgentEmployees(pageParam),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    enabled: canRead,
  });
  const ownWorkQuery = useInfiniteQuery({
    queryKey: operationsQueryKeys.urgentOwnWork,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }: { pageParam: string | null }): Promise<UrgentOwnWorkPageRsp> =>
      listOwnUrgentWork(pageParam),
    getNextPageParam: (lastPage: UrgentOwnWorkPageRsp): string | undefined =>
      lastPage.next_cursor ?? undefined,
    enabled: canRead,
  });
  const teamWorkQuery = useInfiniteQuery({
    queryKey: operationsQueryKeys.urgentTeamWork,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => listTeamUrgentWork(pageParam),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    enabled: canRead && canManagePeers,
  });

  const customers: UrgentWorkCustomer[] = useMemo(
    () => customersQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [customersQuery.data?.pages],
  );
  const employees: UrgentWorkEmployee[] = useMemo(
    () => employeesQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [employeesQuery.data?.pages],
  );

  const ownWork: UrgentWorkItem[] = useMemo<UrgentWorkItem[]>(
    (): UrgentWorkItem[] => ownWorkQuery.data?.pages.flatMap((page: UrgentOwnWorkPageRsp) => page.items) ?? [],
    [ownWorkQuery.data],
  );
  const historyPages: UrgentOwnWorkPageRsp[] = ownWorkQuery.data?.pages ?? [];
  const historyItems: UrgentWorkItem[] = historyPages[historyPage - 1]?.items ?? [];
  const historyHasNext: boolean = historyPage < historyPages.length || ownWorkQuery.hasNextPage;
  const teamWork: UrgentWorkItem[] = useMemo<UrgentWorkItem[]>(
    (): UrgentWorkItem[] => teamWorkQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [teamWorkQuery.data?.pages],
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
    employees.find((employee: UrgentWorkEmployee): boolean => employee.is_self) ?? null;
  const clockableEmployees: UrgentWorkEmployee[] = useMemo<UrgentWorkEmployee[]>(
    (): UrgentWorkEmployee[] => employees,
    [employees],
  );

  useEffect((): void => {
    if (activeOwnWork) {
      setCustomerId(activeOwnWork.claimed_customer_id);
      setSelectedEmployeeIds([]);
      return;
    }
    if (!customerId && customers.length > 0) {
      const firstCustomerId: string | undefined = customers.at(0)?.customer_id;
      if (firstCustomerId) {
        setCustomerId(firstCustomerId);
        setManualCustomerId((current: string): string => current || firstCustomerId);
      }
    }
  }, [activeOwnWork, customers, customerId]);

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

  const manualMutation: UseMutationResult<UrgentWorkItem, unknown, UrgentManualActionInput> = useMutation<
    UrgentWorkItem,
    unknown,
    UrgentManualActionInput
  >({
    mutationFn: (input: UrgentManualActionInput): Promise<UrgentWorkItem> => submitManualUrgentWork(input),
    retry: (failureCount: number, error: unknown): boolean => failureCount < 1 && isRetryableApiError(error),
    onSuccess: (work: UrgentWorkItem): void => {
      setFeedback({
        kind: "success",
        message: `Đã gửi bổ sung ${formatDuration(work.worked_seconds ?? 0)} tại ${work.customer_name}. Quản lý sẽ đối soát trước khi tính lương.`,
      });
      setManualNote("");
      setHistoryPage(1);
      void refreshUrgentWork();
    },
    onError: (error: unknown): void => {
      setFeedback({ kind: "error", message: friendlyApiError(error, "Không thể gửi công việc bổ sung.") });
    },
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

  const submitManual = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    if (!isOnline) {
      setFeedback({ kind: "error", message: "Thiết bị đang ngoại tuyến. Hãy kết nối mạng trước khi gửi." });
      return;
    }
    if (!manualCustomerId || !manualStartedAt || !manualEndedAt) {
      setFeedback({ kind: "error", message: "Hãy nhập đầy đủ nơi làm việc và thời gian." });
      return;
    }
    const startedAt: Date = new Date(manualStartedAt);
    const endedAt: Date = new Date(manualEndedAt);
    if (endedAt <= startedAt) {
      setFeedback({ kind: "error", message: "Thời gian kết thúc phải sau thời gian bắt đầu." });
      return;
    }
    setFeedback(null);
    manualMutation.mutate({
      idempotencyKey: crypto.randomUUID(),
      payload: {
        customer_id: manualCustomerId,
        started_at: startedAt.toISOString(),
        ended_at: endedAt.toISOString(),
        note: manualNote.trim() || null,
      },
    });
  };

  const changeHistoryPage = (nextPage: number): void => {
    if (nextPage < 1) return;
    if (nextPage <= historyPages.length) {
      setHistoryPage(nextPage);
      return;
    }
    if (nextPage === historyPages.length + 1 && ownWorkQuery.hasNextPage) {
      void ownWorkQuery.fetchNextPage().then((result): void => {
        if ((result.data?.pages.length ?? 0) >= nextPage) setHistoryPage(nextPage);
      });
    }
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
        <p className="text-sm font-bold text-blue-100">Ghi nhận công việc của nhân viên</p>
        <h2 className="mt-2 text-2xl font-black">Ghi trực tiếp hoặc bổ sung khi quên chấm công</h2>
        <p className="mt-2 max-w-2xl text-sm leading-6 text-blue-50">
          Khi đang làm, hãy dùng Bắt đầu/Kết thúc để hệ thống ghi thời gian máy chủ. Nếu đã làm nhưng quên thao tác, bạn có thể tự khai lại thời gian; quản lý vẫn phải đối soát trước khi tính lương.
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
                      Bắt đầu {formatDateTime(work.started_at)} · {work.started_by_username} · {sourceLabel(work.start_source)}
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

      <section className="panel p-3 sm:p-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <button className={`min-h-12 rounded-xl px-4 text-sm font-bold ${entryMode === "live" ? "bg-blue-700 text-white" : "bg-slate-100 text-slate-700"}`} onClick={(): void => setEntryMode("live")} type="button"><LogIn className="mr-2 inline size-4" />Ghi trực tiếp</button>
          <button className={`min-h-12 rounded-xl px-4 text-sm font-bold ${entryMode === "manual" ? "bg-violet-700 text-white" : "bg-slate-100 text-slate-700"}`} onClick={(): void => setEntryMode("manual")} type="button"><ClipboardPlus className="mr-2 inline size-4" />Bổ sung ca đã làm</button>
        </div>
      </section>

      {entryMode === "live" ? <form className="panel p-5 sm:p-6" onSubmit={submitStart}>
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
            {customers.map((customer: UrgentWorkCustomer): React.JSX.Element => (
              <option key={customer.customer_id} value={customer.customer_id}>
                {customer.customer_name}
              </option>
            ))}
          </select>
          {customersQuery.hasNextPage ? <button className="mt-2 text-xs font-semibold text-blue-700" disabled={customersQuery.isFetchingNextPage} onClick={() => void customersQuery.fetchNextPage()} type="button">{customersQuery.isFetchingNextPage ? "Đang tải..." : "Tải thêm khách hàng"}</button> : null}
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
          {employeesQuery.hasNextPage ? <button className="mt-3 text-xs font-semibold text-blue-700" disabled={employeesQuery.isFetchingNextPage} onClick={() => void employeesQuery.fetchNextPage()} type="button">{employeesQuery.isFetchingNextPage ? "Đang tải..." : "Tải thêm nhân viên"}</button> : null}
        </div>

        <button
          className="action-primary mt-5 w-full sm:w-auto"
          disabled={!canStart || !isOnline || !customerId || selectedEmployeeIds.length === 0 || startMutation.isPending}
          type="submit"
        >
          {startMutation.isPending ? <RefreshCw className="size-4 animate-spin" /> : <LogIn className="size-4" />}
          {startMutation.isPending ? "Đang ghi nhận..." : `Bắt đầu cho ${selectedEmployeeIds.length} người`}
        </button>
      </form> : <form className="panel p-5 sm:p-6" onSubmit={submitManual}>
        <div className="flex items-start gap-3">
          <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-violet-50 text-violet-700"><ClipboardPlus className="size-5" /></div>
          <div><h2 className="font-bold text-slate-950">Bổ sung công việc đã làm</h2><p className="mt-1 text-sm leading-6 text-slate-500">Chỉ khai công việc của chính bạn. Thông tin này là bằng chứng từ nhân viên, không tự động được duyệt hoặc tính lương.</p></div>
        </div>
        <label className="mt-5 block text-sm font-semibold text-slate-700">Khách hàng / nơi đã làm việc<select className="mt-1.5 min-h-11 w-full rounded-xl border border-slate-200 bg-white px-3" disabled={manualMutation.isPending} onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setManualCustomerId(event.target.value)} required value={manualCustomerId}><option value="">Chọn khách hàng</option>{customers.map((customer: UrgentWorkCustomer): React.JSX.Element => <option key={customer.customer_id} value={customer.customer_id}>{customer.customer_name}</option>)}</select></label>
        <div className="mt-4 grid min-w-0 gap-3 sm:grid-cols-2">
          <label className="min-w-0 text-sm font-semibold text-slate-700">Bắt đầu làm<input className="mt-1.5 min-h-11 w-full min-w-0 rounded-xl border border-slate-200 px-3" disabled={manualMutation.isPending} max={manualEndedAt} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setManualStartedAt(event.target.value)} required step="60" type="datetime-local" value={manualStartedAt} /></label>
          <label className="min-w-0 text-sm font-semibold text-slate-700">Kết thúc làm<input className="mt-1.5 min-h-11 w-full min-w-0 rounded-xl border border-slate-200 px-3" disabled={manualMutation.isPending} min={manualStartedAt} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setManualEndedAt(event.target.value)} required step="60" type="datetime-local" value={manualEndedAt} /></label>
        </div>
        <label className="mt-4 block text-sm font-semibold text-slate-700">Ghi chú <span className="font-normal text-slate-500">(không bắt buộc)</span><textarea className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5" disabled={manualMutation.isPending} maxLength={1000} onChange={(event: React.ChangeEvent<HTMLTextAreaElement>): void => setManualNote(event.target.value)} placeholder="Ví dụ: Quên bấm bắt đầu khi đến nơi làm việc" rows={3} value={manualNote} /></label>
        <div className="mt-4 rounded-xl bg-amber-50 px-4 py-3 text-sm leading-6 text-amber-900">Thời gian do bạn tự khai sẽ được giữ nguyên làm bằng chứng. Quản lý đối chiếu với thông tin khách hàng và chốt kết quả cuối cùng.</div>
        <button className="action-primary mt-5 w-full sm:w-auto" disabled={!canStart || !isOnline || !manualCustomerId || !manualStartedAt || !manualEndedAt || manualMutation.isPending} type="submit">{manualMutation.isPending ? <RefreshCw className="size-4 animate-spin" /> : <ClipboardPlus className="size-4" />}{manualMutation.isPending ? "Đang gửi..." : "Gửi công việc bổ sung"}</button>
      </form>}

      {canManagePeers && teamWorkQuery.hasNextPage ? <div className="flex justify-center"><button className="action-secondary" disabled={teamWorkQuery.isFetchingNextPage} onClick={() => void teamWorkQuery.fetchNextPage()} type="button">{teamWorkQuery.isFetchingNextPage ? "Đang tải..." : "Tải thêm công việc cùng nhóm"}</button></div> : null}

      {historyItems.length > 0 ? (
        <section className="panel overflow-hidden">
          <div className="border-b border-slate-200 px-5 py-4">
            <h2 className="font-bold text-slate-950">Lịch sử của tôi</h2>
            <p className="mt-1 text-sm text-slate-500">
              Mỗi trang hiển thị theo giới hạn của hệ thống. Công việc bổ sung được đánh dấu riêng với thời điểm gửi do máy chủ ghi nhận.
            </p>
          </div>
          <div className="divide-y divide-slate-100">
            {historyItems.map((work: UrgentWorkItem): React.JSX.Element => (
              <article className="px-5 py-5" key={work.report_id}>
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <p className="flex items-center gap-2 font-bold text-slate-950">
                      <MapPin className="size-4 text-blue-600" />
                      {work.customer_name}
                    </p>
                    <p className="mt-1 text-xs font-semibold text-slate-500">
                      Chi nhánh: {work.branch_name}
                    </p>
                    {work.submission_kind === "manual" ? <span className="mt-2 inline-flex rounded-full bg-amber-100 px-2.5 py-1 text-[11px] font-bold text-amber-800">Nhân viên tự khai bổ sung · gửi {formatDateTime(work.created_at)}</span> : <span className="mt-2 inline-flex rounded-full bg-blue-50 px-2.5 py-1 text-[11px] font-bold text-blue-700">Ghi trực tiếp tại nơi làm việc</span>}
                  </div>
                  <p className="flex items-center gap-2 rounded-full bg-violet-50 px-3 py-1.5 text-sm font-bold text-violet-800">
                    <Clock3 className="size-4 text-violet-600" />
                    {work.worked_seconds === null ? "Đang làm" : formatDuration(work.worked_seconds)}
                  </p>
                </div>

                <dl className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                  <div className="rounded-xl bg-emerald-50 p-3">
                    <dt className="text-[11px] font-bold uppercase tracking-wide text-emerald-700">{work.submission_kind === "manual" ? "Bắt đầu tự khai" : "Check-in"}</dt>
                    <dd className="mt-1 text-sm font-bold text-emerald-950">
                      {formatDateTime(work.started_at)}
                    </dd>
                    <dd className="mt-1 text-xs text-emerald-800">
                      {work.submission_kind === "manual" ? `Khai bởi ${work.started_by_username}` : `Bấm bởi ${work.started_by_username} · ${sourceLabel(work.start_source)}`}
                    </dd>
                  </div>
                  <div className="rounded-xl bg-amber-50 p-3">
                    <dt className="text-[11px] font-bold uppercase tracking-wide text-amber-700">{work.submission_kind === "manual" ? "Kết thúc tự khai" : "Check-out"}</dt>
                    <dd className="mt-1 text-sm font-bold text-amber-950">
                      {work.ended_at === null ? "Chưa kết thúc" : formatDateTime(work.ended_at)}
                    </dd>
                    <dd className="mt-1 text-xs text-amber-800">
                      {work.submission_kind === "manual"
                        ? `Khai bởi ${work.ended_by_username ?? work.started_by_username}`
                        : work.ended_by_username === null
                        ? "Chưa có người kết thúc"
                        : `Bấm bởi ${work.ended_by_username} · ${sourceLabel(work.end_source)}`}
                    </dd>
                  </div>
                  <div className="rounded-xl bg-slate-50 p-3 sm:col-span-2">
                    <dt className="text-[11px] font-bold uppercase tracking-wide text-slate-500">Khoảng làm việc</dt>
                    <dd className="mt-1 text-sm font-bold text-slate-900">
                      {formatDateTime(work.started_at)} → {work.ended_at === null ? "đang làm" : formatDateTime(work.ended_at)}
                    </dd>
                    <dd className="mt-1 text-xs text-slate-600">
                      Tổng thời gian: {work.worked_seconds === null ? "chưa hoàn tất" : formatDuration(work.worked_seconds)}
                    </dd>
                  </div>
                </dl>
                {work.staff_note ? <p className="mt-3 rounded-xl bg-violet-50 px-3 py-2 text-sm text-violet-900"><span className="font-bold">Ghi chú của nhân viên:</span> {work.staff_note}</p> : null}
              </article>
            ))}
          </div>
          <ReconciliationPagination currentItemCount={historyItems.length} currentPage={historyPage} hasNextPage={historyHasNext} nextPagePending={ownWorkQuery.isFetchingNextPage} onPageChange={changeHistoryPage} />
        </section>
      ) : null}
    </div>
  );
}
