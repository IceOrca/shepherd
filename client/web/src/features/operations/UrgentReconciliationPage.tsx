import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { CheckCircle2, CircleAlert, GitCompareArrows, MapPin, RefreshCw, Save } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import type {
  JobPosition,
  ManualRateOverrideRequest,
  PermissionCode,
  ReconciliationStatus,
  UrgentCustomerWorkRecord,
  UrgentWorkFacility,
  UrgentWorkReconciliation,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { formatDateTime, formatDuration } from "../../shared/lib/format";
import { useAuth } from "../auth/AuthProvider";
import {
  listJobs,
  listUrgentFacilities,
  listUrgentReconciliations,
  operationsQueryKeys,
  reconcileUrgentWork,
  saveUrgentCustomerWorkRecord,
} from "./api";

interface EvidenceDraft {
  facilityId: string;
  startedAt: string;
  endedAt: string;
  reference: string;
  notes: string;
}

interface FinalDraft {
  facilityId: string;
  jobId: string;
  hours: string;
  reason: string;
  useManualRate: boolean;
  currency: string;
  billRate: string;
  workerRate: string;
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
    facilityId: customerRecord?.confirmed_customer_facility_id ?? "",
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
    facilityId:
      item.final_customer_facility_id ??
      item.customer_record?.confirmed_customer_facility_id ??
      item.work.claimed_customer_facility_id,
    jobId: item.final_job_id ?? "",
    hours: seconds > 0 ? (seconds / 3600).toFixed(2) : "",
    reason: item.adjustment_reason ?? "",
    useManualRate: false,
    currency: "VND",
    billRate: "",
    workerRate: "",
  };
}

export function UrgentReconciliationPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const queryClient: QueryClient = useQueryClient();
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canRead: boolean = permissions.includes("business.reconciliation.read");
  const canManage: boolean = permissions.includes("business.urgent_work.reconcile");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [evidence, setEvidence] = useState<EvidenceDraft>({
    facilityId: "",
    startedAt: "",
    endedAt: "",
    reference: "",
    notes: "",
  });
  const [finalDraft, setFinalDraft] = useState<FinalDraft>({
    facilityId: "",
    jobId: "",
    hours: "",
    reason: "",
    useManualRate: false,
    currency: "VND",
    billRate: "",
    workerRate: "",
  });
  const [message, setMessage] = useState<string | null>(null);

  const reconciliationQuery: UseQueryResult<UrgentWorkReconciliation[], Error> = useQuery({
    queryKey: operationsQueryKeys.urgentReconciliations,
    queryFn: listUrgentReconciliations,
    enabled: canRead,
  });
  const facilitiesQuery: UseQueryResult<UrgentWorkFacility[], Error> = useQuery({
    queryKey: operationsQueryKeys.urgentFacilities,
    queryFn: listUrgentFacilities,
    enabled: canRead,
  });
  const jobsQuery: UseQueryResult<JobPosition[], Error> = useQuery({
    queryKey: operationsQueryKeys.jobs,
    queryFn: listJobs,
    enabled: canRead,
  });

  const items: UrgentWorkReconciliation[] = useMemo<UrgentWorkReconciliation[]>(
    (): UrgentWorkReconciliation[] => reconciliationQuery.data ?? [],
    [reconciliationQuery.data],
  );
  const selected: UrgentWorkReconciliation | null =
    items.find((item: UrgentWorkReconciliation): boolean => item.work.report_id === selectedId) ?? null;

  useEffect((): void => {
    if (!selectedId) {
      const firstItem: UrgentWorkReconciliation | undefined = items.at(0);
      if (firstItem) {
        setSelectedId(firstItem.work.report_id);
      }
    }
  }, [items, selectedId]);

  useEffect((): void => {
    if (!selected) {
      return;
    }
    setEvidence(initialEvidence(selected));
    setFinalDraft(initialFinal(selected));
    setMessage(null);
  }, [selected]);

  const refresh = (): Promise<void> =>
    queryClient.invalidateQueries({ queryKey: operationsQueryKeys.urgentReconciliations });

  const evidenceMutation: UseMutationResult<UrgentCustomerWorkRecord, unknown, void> = useMutation<
    UrgentCustomerWorkRecord,
    unknown,
    void
  >({
    mutationFn: (): Promise<UrgentCustomerWorkRecord> => {
      if (!selectedId) {
        return Promise.reject(new Error("urgent report is not selected"));
      }
      return saveUrgentCustomerWorkRecord(selectedId, {
        confirmed_customer_facility_id: evidence.facilityId,
        confirmed_started_at: new Date(evidence.startedAt).toISOString(),
        confirmed_ended_at: new Date(evidence.endedAt).toISOString(),
        customer_reference: evidence.reference.trim() || null,
        notes: evidence.notes.trim() || null,
      });
    },
    onSuccess: (): void => {
      setMessage("Đã lưu bằng chứng độc lập từ khách hàng.");
      void refresh();
    },
    onError: (error: unknown): void => {
      setMessage(friendlyApiError(error, "Không thể lưu bằng chứng khách hàng."));
    },
  });

  const reconcileMutation: UseMutationResult<UrgentWorkReconciliation, unknown, void> = useMutation<
    UrgentWorkReconciliation,
    unknown,
    void
  >({
    mutationFn: (): Promise<UrgentWorkReconciliation> => {
      if (!selectedId) {
        return Promise.reject(new Error("urgent report is not selected"));
      }
      const manualRate: ManualRateOverrideRequest | null = finalDraft.useManualRate
        ? {
            currency: finalDraft.currency.trim().toUpperCase(),
            bill_hourly_rate: finalDraft.billRate.trim(),
            worker_hourly_rate: finalDraft.workerRate.trim(),
          }
        : null;
      return reconcileUrgentWork(selectedId, {
        final_customer_facility_id: finalDraft.facilityId,
        job_id: finalDraft.jobId,
        worked_seconds: Math.round(Number(finalDraft.hours) * 3600),
        adjustment_reason: finalDraft.reason.trim() || null,
        manual_rate: manualRate,
      });
    },
    onSuccess: (): void => {
      setMessage("Đã chốt cơ sở, thời gian và ảnh chụp tài chính cuối cùng.");
      void refresh();
    },
    onError: (error: unknown): void => {
      setMessage(
        friendlyApiError(error, "Không thể chốt đối soát. Mọi chênh lệch về cơ sở hoặc thời gian cần có lý do."),
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

  const isPending: boolean = reconciliationQuery.isPending || facilitiesQuery.isPending || jobsQuery.isPending;
  const firstError: unknown = reconciliationQuery.error ?? facilitiesQuery.error ?? jobsQuery.error;
  if (isPending) {
    return (
      <section className="panel p-8 text-center text-sm text-slate-500">
        <RefreshCw className="mr-2 inline size-4 animate-spin" />
        Đang tải dữ liệu đối soát khẩn...
      </section>
    );
  }
  if (firstError) {
    return (
      <section className="panel p-8 text-center text-sm text-red-600">
        <CircleAlert className="mr-2 inline size-4" />
        {friendlyApiError(firstError, "Không thể tải dữ liệu đối soát khẩn.")}
      </section>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-blue-100 bg-blue-50 px-4 py-3 text-sm text-blue-900">
        <p><strong>Mặc định:</strong> đối soát công việc phát sinh không có ca tạo trước.</p>
        <Link className="font-bold text-blue-700 hover:underline" to="/operations/reconciliation/planned">
          Mở đối soát ca kế hoạch
        </Link>
      </div>

      <div className="grid gap-5 xl:grid-cols-[minmax(300px,0.8fr)_minmax(0,1.2fr)]">
        <section className="panel overflow-hidden">
          <div className="border-b border-slate-200 px-5 py-4">
            <h2 className="font-bold text-slate-950">Công việc cần đối soát</h2>
            <p className="mt-1 text-sm text-slate-500">Bằng chứng nhân viên chọn cơ sở và tự/ghi hộ thời gian.</p>
          </div>
          <div className="max-h-[72vh] divide-y divide-slate-100 overflow-y-auto">
            {items.map((item: UrgentWorkReconciliation): React.JSX.Element => (
              <button
                className={`w-full px-5 py-4 text-left hover:bg-slate-50 ${selectedId === item.work.report_id ? "bg-blue-50" : ""}`}
                key={item.work.report_id}
                onClick={(): void => setSelectedId(item.work.report_id)}
                type="button"
              >
                <div className="flex items-start justify-between gap-2">
                  <div>
                    <p className="font-bold text-slate-900">{item.work.employee_name}</p>
                    <p className="mt-1 text-xs text-slate-500">
                      {item.work.customer_name} · {item.work.claimed_facility_name}
                    </p>
                  </div>
                  <span className={`shrink-0 rounded-full px-2.5 py-1 text-[11px] font-bold ${statusTone(item.reconciliation_status)}`}>
                    {statusLabel(item.reconciliation_status)}
                  </span>
                </div>
                <p className="mt-2 text-xs text-slate-500">{formatDateTime(item.work.started_at)}</p>
              </button>
            ))}
            {items.length === 0 ? (
              <p className="p-8 text-center text-sm text-slate-500">Chưa có công việc khẩn cần đối soát.</p>
            ) : null}
          </div>
        </section>

        {selected ? (
          <div className="space-y-5">
            {message ? (
              <div className="rounded-xl border border-blue-200 bg-blue-50 px-4 py-3 text-sm font-medium text-blue-800">
                {message}
              </div>
            ) : null}

            <section className="panel p-5 sm:p-6">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h2 className="text-lg font-black text-slate-950">{selected.work.employee_name}</h2>
                  <p className="mt-1 text-sm text-slate-500">
                    {selected.work.employee_code} · {selected.work.customer_name}
                  </p>
                </div>
                <span className={`rounded-full px-3 py-1 text-xs font-bold ${statusTone(selected.reconciliation_status)}`}>
                  {statusLabel(selected.reconciliation_status)}
                </span>
              </div>
              <div className="mt-5 grid gap-3 lg:grid-cols-3">
                <div className="rounded-xl bg-violet-50 p-4">
                  <p className="text-xs font-bold uppercase text-violet-600">Nhân viên khai</p>
                  <p className="mt-2 font-black text-violet-950">{selected.work.claimed_facility_name}</p>
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
                    {selected.customer_record?.confirmed_facility_name ?? "Chưa nhập"}
                  </p>
                  <p className="mt-1 text-sm font-bold text-amber-900">
                    {selected.customer_record ? formatDuration(selected.customer_record.confirmed_worked_seconds) : "—"}
                  </p>
                  <p className="mt-1 text-xs text-amber-700">Nguồn độc lập, không sao chép từ nhân viên</p>
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

            <form className="panel p-5 sm:p-6" onSubmit={saveEvidence}>
              <div className="flex items-center gap-2">
                <MapPin className="size-5 text-amber-600" />
                <h3 className="font-bold text-slate-950">Xác nhận / bill từ khách hàng</h3>
              </div>
              <p className="mt-1 text-sm text-slate-500">
                Nhập đúng cơ sở và thời gian khách hàng cung cấp. Form trống cho đến khi có bằng chứng khách hàng.
              </p>
              <label className="mt-4 block text-sm font-semibold text-slate-700">
                Cơ sở khách hàng xác nhận
                <select
                  className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
                  disabled={!canManage || selected.reconciliation_status === "reconciled"}
                  onChange={(event: React.ChangeEvent<HTMLSelectElement>): void =>
                    setEvidence((current: EvidenceDraft): EvidenceDraft => ({ ...current, facilityId: event.target.value }))
                  }
                  required
                  value={evidence.facilityId}
                >
                  <option value="">Chọn cơ sở theo bill</option>
                  {(facilitiesQuery.data ?? []).map((facility): React.JSX.Element => (
                    <option key={facility.facility_id} value={facility.facility_id}>
                      {facility.customer_name} · {facility.facility_name}
                    </option>
                  ))}
                </select>
              </label>
              <div className="mt-3 grid gap-3 sm:grid-cols-2">
                <label className="text-sm font-semibold text-slate-700">
                  Bắt đầu xác nhận
                  <input
                    className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5"
                    disabled={!canManage || selected.reconciliation_status === "reconciled"}
                    onChange={(event: React.ChangeEvent<HTMLInputElement>): void =>
                      setEvidence((current: EvidenceDraft): EvidenceDraft => ({ ...current, startedAt: event.target.value }))
                    }
                    required
                    type="datetime-local"
                    value={evidence.startedAt}
                  />
                </label>
                <label className="text-sm font-semibold text-slate-700">
                  Kết thúc xác nhận
                  <input
                    className="mt-1.5 w-full rounded-xl border border-slate-200 px-3 py-2.5"
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
                <button className="action-secondary mt-4" disabled={!canManage || evidenceMutation.isPending} type="submit">
                  <Save className="size-4" />
                  {evidenceMutation.isPending ? "Đang lưu..." : "Lưu bằng chứng khách hàng"}
                </button>
              ) : null}
            </form>

            <section className="panel p-5 sm:p-6">
              <div className="flex items-center gap-2">
                <GitCompareArrows className="size-5 text-blue-600" />
                <h3 className="font-bold text-slate-950">Chốt đối soát</h3>
              </div>
              <div className="mt-4 grid gap-3 sm:grid-cols-2">
                <label className="text-sm font-semibold text-slate-700">
                  Cơ sở cuối cùng
                  <select
                    className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
                    disabled={selected.reconciliation_status === "reconciled"}
                    onChange={(event: React.ChangeEvent<HTMLSelectElement>): void =>
                      setFinalDraft((current: FinalDraft): FinalDraft => ({ ...current, facilityId: event.target.value }))
                    }
                    value={finalDraft.facilityId}
                  >
                    <option value="">Chọn cơ sở cuối</option>
                    {(facilitiesQuery.data ?? []).map((facility): React.JSX.Element => (
                      <option key={facility.facility_id} value={facility.facility_id}>
                        {facility.customer_name} · {facility.facility_name}
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
                    {(jobsQuery.data ?? []).map((job: JobPosition): React.JSX.Element => (
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
                    placeholder="Bắt buộc nếu cơ sở hoặc thời gian lệch"
                    value={finalDraft.reason}
                  />
                </label>
              </div>

              <label className="mt-4 flex items-center gap-3 text-sm font-semibold text-slate-700">
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
                  className="action-primary mt-4"
                  disabled={
                    !canManage ||
                    !selected.customer_record ||
                    !finalDraft.facilityId ||
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
