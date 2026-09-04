import { ClipboardCheck } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { PLANNED_STAFFING_ENABLED } from "../../shared/lib/features";

export type ReconciliationMode = "urgent" | "planned";

export function ReconciliationModeSelector({ mode }: { mode: ReconciliationMode }): React.JSX.Element {
  const navigate: ReturnType<typeof useNavigate> = useNavigate();

  if (!PLANNED_STAFFING_ENABLED) {
    return <></>;
  }

  return (
    <section className="panel p-4 sm:p-5">
      <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(260px,0.7fr)] sm:items-end">
        <div className="flex min-w-0 items-start gap-3">
          <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-blue-50 text-blue-700">
            <ClipboardCheck className="size-5" />
          </div>
          <div className="min-w-0">
            <h2 className="font-black text-slate-950">Loại công việc đối soát</h2>
            <p className="mt-1 text-sm leading-5 text-slate-500">
              Chọn công việc phát sinh tại nơi làm hoặc ca đã được lên kế hoạch trước.
            </p>
          </div>
        </div>
        <label className="min-w-0 text-sm font-semibold text-slate-700">
          Phạm vi công việc
          <select
            aria-label="Loại công việc đối soát"
            className="mt-1.5 min-h-11 w-full rounded-xl border-slate-300 bg-white px-3 text-sm font-semibold text-slate-800"
            onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => {
              const nextMode: ReconciliationMode = event.target.value as ReconciliationMode;
              navigate(nextMode === "planned" ? "/operations/reconciliation/planned" : "/operations/reconciliation");
            }}
            value={mode}
          >
            <option value="urgent">Công việc phát sinh (không tạo ca trước)</option>
            <option value="planned">Ca kế hoạch (đã tạo trước)</option>
          </select>
        </label>
      </div>
    </section>
  );
}
