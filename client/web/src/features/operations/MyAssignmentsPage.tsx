import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckCircle2,
  CircleAlert,
  Clock3,
  LocateFixed,
  LogIn,
  LogOut,
  MapPin,
  RefreshCw,
  ShieldCheck,
  TimerReset,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { OwnStaffingAssignment, ShiftWorkActionRequest, ShiftWorkSession } from "../../api/generated/contracts";
import { friendlyApiError, isRetryableApiError } from "../../shared/api/client";
import { assignmentStatusLabel, formatDateTime, formatDuration } from "../../shared/lib/format";
import { useOnlineStatus } from "../../shared/lib/useOnlineStatus";
import { useAuth } from "../auth/AuthProvider";
import { executeWorkAction, listOwnAssignments, operationsQueryKeys, type WorkActionInput } from "./api";

interface ActionVariables extends WorkActionInput {
  locationWarning: string | null;
}

interface Feedback {
  kind: "success" | "warning" | "error";
  message: string;
}

function assignmentTone(assignment: OwnStaffingAssignment): string {
  if (assignment.is_working) {
    return "bg-emerald-50 text-emerald-700 ring-emerald-600/10";
  }
  switch (assignment.status) {
    case "assigned":
      return "bg-blue-50 text-blue-700 ring-blue-600/10";
    case "approved":
      return "bg-slate-100 text-slate-600 ring-slate-500/10";
    case "cancelled":
      return "bg-red-50 text-red-700 ring-red-600/10";
  }
}

function getLocation(enabled: boolean): Promise<{ payload: ShiftWorkActionRequest; warning: string | null }> {
  if (!enabled) {
    return Promise.resolve({ payload: {}, warning: null });
  }
  if (!("geolocation" in navigator)) {
    return Promise.resolve({ payload: {}, warning: "Thiết bị không hỗ trợ gửi vị trí." });
  }

  return new Promise((resolve) => {
    navigator.geolocation.getCurrentPosition(
      (position) => {
        resolve({
          payload: {
            latitude: position.coords.latitude,
            longitude: position.coords.longitude,
            accuracy_meters: position.coords.accuracy,
          },
          warning: null,
        });
      },
      () => resolve({ payload: {}, warning: "Không lấy được vị trí; thời gian vẫn được máy chủ ghi nhận." }),
      { enableHighAccuracy: true, maximumAge: 30_000, timeout: 8_000 },
    );
  });
}

export function MyAssignmentsPage() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const isOnline = useOnlineStatus();
  const permissions = auth.profile?.permissions ?? [];
  const canRead = permissions.includes("business.staffing_work.self.read");
  const canManage = permissions.includes("business.staffing_work.self.manage");
  const [shareLocation, setShareLocation] = useState(true);
  const [preparingAssignmentId, setPreparingAssignmentId] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<Feedback | null>(null);

  const assignmentsQuery = useQuery({
    queryKey: operationsQueryKeys.ownAssignments,
    queryFn: listOwnAssignments,
    enabled: canRead,
  });

  const actionMutation = useMutation<ShiftWorkSession, unknown, ActionVariables>({
    mutationFn: (variables) => executeWorkAction(variables),
    retry: (failureCount, error) => failureCount < 1 && isRetryableApiError(error),
    onSuccess: (_session, variables) => {
      const actionText = variables.action === "start" ? "bắt đầu" : "kết thúc";
      setFeedback({
        kind: variables.locationWarning ? "warning" : "success",
        message: variables.locationWarning
          ? `Đã ${actionText} ca. ${variables.locationWarning}`
          : `Đã ${actionText} ca thành công. Thời gian được ghi nhận theo máy chủ.`,
      });
      void queryClient.invalidateQueries({ queryKey: operationsQueryKeys.all });
    },
    onError: (error) => {
      setFeedback({
        kind: "error",
        message: friendlyApiError(error, "Không thể ghi nhận ca làm. Vui lòng kiểm tra và thử lại."),
      });
    },
    onSettled: () => setPreparingAssignmentId(null),
  });

  const assignments = useMemo(
    () =>
      [...(assignmentsQuery.data ?? [])].sort((left, right) => {
        if (left.is_working !== right.is_working) {
          return left.is_working ? -1 : 1;
        }
        return new Date(left.starts_at).getTime() - new Date(right.starts_at).getTime();
      }),
    [assignmentsQuery.data],
  );

  const runAction = async (assignment: OwnStaffingAssignment, action: "start" | "end") => {
    if (!isOnline) {
      setFeedback({ kind: "error", message: "Thiết bị đang ngoại tuyến. Hãy kết nối mạng trước khi ghi nhận." });
      return;
    }

    setFeedback(null);
    setPreparingAssignmentId(assignment.assignment_id);
    const location = await getLocation(shareLocation);
    actionMutation.mutate({
      action,
      assignmentId: assignment.assignment_id,
      idempotencyKey: crypto.randomUUID(),
      payload: location.payload,
      locationWarning: location.warning,
    });
  };

  if (!canRead) {
    return (
      <section className="panel p-8 text-center">
        <ShieldCheck className="mx-auto size-10 text-slate-400" />
        <h2 className="mt-4 text-lg font-bold text-slate-950">Chưa có quyền xem ca làm</h2>
        <p className="mt-2 text-sm text-slate-500">Vui lòng liên hệ người quản lý để được cấp quyền phù hợp.</p>
      </section>
    );
  }

  return (
    <div className="space-y-5">
      <section className="panel flex flex-col gap-4 p-5 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-start gap-3">
          <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-blue-50 text-blue-700">
            <LocateFixed className="size-5" />
          </div>
          <div>
            <h2 className="font-bold text-slate-950">Gửi vị trí khi ghi nhận</h2>
            <p className="mt-1 max-w-2xl text-sm leading-6 text-slate-500">
              Vị trí giúp quản lý xác nhận nơi làm việc. Nếu GPS lỗi, hệ thống vẫn dùng thời gian máy chủ.
            </p>
          </div>
        </div>
        <label className="flex min-h-11 shrink-0 cursor-pointer items-center justify-between gap-4 rounded-xl bg-slate-50 px-4 text-sm font-bold text-slate-700 sm:justify-start">
          <span>{shareLocation ? "Đang bật" : "Đang tắt"}</span>
          <input
            checked={shareLocation}
            className="peer sr-only"
            onChange={(event) => setShareLocation(event.target.checked)}
            type="checkbox"
          />
          <span className="relative h-6 w-11 rounded-full bg-slate-300 transition peer-checked:bg-blue-600 after:absolute after:left-1 after:top-1 after:size-4 after:rounded-full after:bg-white after:transition peer-checked:after:translate-x-5" />
        </label>
      </section>

      {feedback ? (
        <div
          className={`flex items-start gap-3 rounded-2xl border px-4 py-3 text-sm font-medium ${
            feedback.kind === "success"
              ? "border-emerald-200 bg-emerald-50 text-emerald-800"
              : feedback.kind === "warning"
                ? "border-amber-200 bg-amber-50 text-amber-800"
                : "border-red-200 bg-red-50 text-red-800"
          }`}
        >
          {feedback.kind === "success" ? <CheckCircle2 className="mt-0.5 size-5 shrink-0" /> : <CircleAlert className="mt-0.5 size-5 shrink-0" />}
          <span>{feedback.message}</span>
        </div>
      ) : null}

      {assignmentsQuery.isPending ? (
        <div className="grid gap-4 xl:grid-cols-2">
          {Array.from({ length: 4 }, (_, index) => (
            <div className="panel p-5" key={index}>
              <div className="skeleton h-5 w-52" />
              <div className="skeleton mt-4 h-4 w-full" />
              <div className="skeleton mt-3 h-12 w-full" />
            </div>
          ))}
        </div>
      ) : null}

      {assignmentsQuery.error ? (
        <section className="panel p-8 text-center">
          <CircleAlert className="mx-auto size-10 text-red-500" />
          <h2 className="mt-4 text-lg font-bold text-slate-950">Chưa thể tải ca làm</h2>
          <p className="mt-2 text-sm text-slate-500">
            {friendlyApiError(assignmentsQuery.error, "Máy chủ chưa thể trả về danh sách ca làm.")}
          </p>
          <button className="action-secondary mt-5" onClick={() => void assignmentsQuery.refetch()} type="button">
            <RefreshCw className="size-4" />
            Thử tải lại
          </button>
        </section>
      ) : null}

      {!assignmentsQuery.isPending && !assignmentsQuery.error && assignments.length === 0 ? (
        <section className="panel px-6 py-14 text-center">
          <TimerReset className="mx-auto size-11 text-slate-300" />
          <h2 className="mt-4 text-lg font-bold text-slate-950">Bạn chưa có ca làm nào</h2>
          <p className="mt-2 text-sm text-slate-500">Ca mới do điều phối viên phân công sẽ xuất hiện tại đây.</p>
        </section>
      ) : null}

      {assignments.length > 0 ? (
        <div className="grid gap-4 xl:grid-cols-2">
          {assignments.map((assignment) => {
            const isBusy = preparingAssignmentId === assignment.assignment_id || (actionMutation.isPending && actionMutation.variables?.assignmentId === assignment.assignment_id);
            const canAct = canManage && assignment.status === "assigned";

            return (
              <article className={`panel overflow-hidden ${assignment.is_working ? "ring-2 ring-emerald-500/30" : ""}`} key={assignment.assignment_id}>
                <div className="p-5 sm:p-6">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                      <h2 className="truncate text-lg font-black text-slate-950">{assignment.customer_name}</h2>
                      <p className="mt-1 flex items-center gap-1.5 text-sm text-slate-500">
                        <MapPin className="size-4 shrink-0 text-blue-600" />
                        <span className="truncate">{assignment.customer_facility_name}</span>
                      </p>
                    </div>
                    <span className={`rounded-full px-3 py-1 text-xs font-bold ring-1 ring-inset ${assignmentTone(assignment)}`}>
                      {assignment.is_working ? "Đang làm việc" : assignmentStatusLabel(assignment.status)}
                    </span>
                  </div>

                  <div className="mt-5 grid gap-3 rounded-xl bg-slate-50 p-4 sm:grid-cols-2">
                    <div>
                      <p className="text-xs font-semibold uppercase tracking-wide text-slate-400">Bắt đầu dự kiến</p>
                      <p className="mt-1 text-sm font-bold text-slate-800">{formatDateTime(assignment.starts_at)}</p>
                    </div>
                    <div>
                      <p className="text-xs font-semibold uppercase tracking-wide text-slate-400">Kết thúc dự kiến</p>
                      <p className="mt-1 text-sm font-bold text-slate-800">{formatDateTime(assignment.ends_at)}</p>
                    </div>
                  </div>

                  <div className="mt-4 flex items-center justify-between gap-4">
                    <div className="flex items-center gap-2 text-sm text-slate-500">
                      <Clock3 className="size-4 text-violet-600" />
                      Đã ghi nhận <strong className="text-slate-800">{formatDuration(assignment.observed_worked_seconds)}</strong>
                    </div>
                  </div>
                </div>

                <div className="border-t border-slate-100 bg-slate-50/70 p-4 sm:px-6">
                  {canAct ? (
                    <button
                      className={`w-full ${assignment.is_working ? "action-secondary" : "action-primary"}`}
                      disabled={!isOnline || isBusy || actionMutation.isPending}
                      onClick={() => void runAction(assignment, assignment.is_working ? "end" : "start")}
                      type="button"
                    >
                      {isBusy ? (
                        <RefreshCw className="size-4 animate-spin" />
                      ) : assignment.is_working ? (
                        <LogOut className="size-4" />
                      ) : (
                        <LogIn className="size-4" />
                      )}
                      {isBusy ? "Đang ghi nhận..." : assignment.is_working ? "Kết thúc ca" : "Bắt đầu ca"}
                    </button>
                  ) : (
                    <p className="text-center text-sm font-medium text-slate-500">
                      {assignment.status === "approved" ? "Ca đã được quản lý duyệt." : assignment.status === "cancelled" ? "Ca này đã bị hủy." : "Bạn chỉ có quyền xem ca này."}
                    </p>
                  )}
                </div>
              </article>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
