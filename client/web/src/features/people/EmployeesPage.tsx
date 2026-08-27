import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { Eye, LoaderCircle, Pencil, RefreshCw, Search, ShieldCheck, UserRound, X } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import type {
  Employee,
  EmployeeCitizenIdUpdateRequest,
  EmployeeSensitiveProfile,
  EmployeeStatus,
  EmployeeUpsertRequest,
  Gender,
  PermissionCode,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { useAuth } from "../auth/AuthProvider";
import {
  getEmployeeCitizenId,
  listEmployees,
  peopleQueryKeys,
  updateEmployee,
  updateEmployeeCitizenId,
} from "./api";

interface EmployeeSaveVariables {
  employeeId: string;
  payload: EmployeeUpsertRequest;
}

interface CitizenIdSaveVariables {
  employeeId: string;
  payload: EmployeeCitizenIdUpdateRequest;
}

function employeeDraft(employee: Employee): EmployeeUpsertRequest {
  return {
    account_id: employee.account_id,
    employee_code: employee.employee_code,
    display_name: employee.display_name,
    legal_first_name: employee.legal_first_name,
    legal_middle_name: employee.legal_middle_name,
    legal_last_name: employee.legal_last_name,
    personal_phone_e164: employee.personal_phone_e164,
    gender: employee.gender,
    status: employee.status,
    hire_date: employee.hire_date,
    termination_date: employee.termination_date,
    expected_version: employee.version,
  };
}

function optionalValue(value: string): string | null {
  return value.trim() || null;
}

function genderLabel(gender: Gender | null): string {
  switch (gender) {
    case "female": return "Nữ";
    case "male": return "Nam";
    case "other": return "Khác";
    case "unspecified": return "Không công bố";
    default: return "Chưa cập nhật";
  }
}

function statusLabel(status: EmployeeStatus): string {
  if (status === "active") return "Đang làm việc";
  if (status === "on_leave") return "Đang nghỉ";
  return "Đã nghỉ việc";
}

export function EmployeesPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const queryClient: ReturnType<typeof useQueryClient> = useQueryClient();
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canRead: boolean = permissions.includes("hr.employees.read");
  const canManage: boolean = permissions.includes("hr.employees.manage");
  const canReadSensitive: boolean = permissions.includes("hr.employees.sensitive.read");
  const canManageSensitive: boolean = permissions.includes("hr.employees.sensitive.manage");
  const [search, setSearch] = useState<string>("");
  const [editingEmployee, setEditingEmployee] = useState<Employee | null>(null);
  const [draft, setDraft] = useState<EmployeeUpsertRequest | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [sensitive, setSensitive] = useState<EmployeeSensitiveProfile | null>(null);
  const [citizenCountry, setCitizenCountry] = useState<string>("VN");
  const [citizenId, setCitizenId] = useState<string>("");
  const [sensitiveLoading, setSensitiveLoading] = useState<boolean>(false);
  const [sensitiveError, setSensitiveError] = useState<string | null>(null);

  const employeesQuery: UseQueryResult<Employee[], Error> = useQuery({
    queryKey: peopleQueryKeys.employees,
    queryFn: listEmployees,
    enabled: canRead,
  });

  const visibleEmployees: Employee[] = useMemo((): Employee[] => {
    const needle: string = search.trim().toLocaleLowerCase("vi");
    if (!needle) return employeesQuery.data ?? [];
    return (employeesQuery.data ?? []).filter((employee: Employee): boolean =>
      [
        employee.employee_code,
        employee.display_name,
        employee.legal_first_name ?? "",
        employee.legal_middle_name ?? "",
        employee.legal_last_name ?? "",
        employee.personal_phone_e164 ?? "",
      ].join(" ").toLocaleLowerCase("vi").includes(needle),
    );
  }, [employeesQuery.data, search]);

  const saveMutation: UseMutationResult<Employee, Error, EmployeeSaveVariables> = useMutation({
    mutationFn: ({ employeeId, payload }: EmployeeSaveVariables): Promise<Employee> =>
      updateEmployee(employeeId, payload),
    onSuccess: (employee: Employee): void => {
      void queryClient.invalidateQueries({ queryKey: peopleQueryKeys.employees });
      setEditingEmployee(employee);
      setDraft(employeeDraft(employee));
      setSensitive((current: EmployeeSensitiveProfile | null): EmployeeSensitiveProfile | null =>
        current ? { ...current, version: employee.version } : null,
      );
      setFeedback(`Đã cập nhật hồ sơ ${employee.display_name}.`);
    },
  });

  const citizenMutation: UseMutationResult<EmployeeSensitiveProfile, Error, CitizenIdSaveVariables> = useMutation({
    mutationFn: ({ employeeId, payload }: CitizenIdSaveVariables): Promise<EmployeeSensitiveProfile> =>
      updateEmployeeCitizenId(employeeId, payload),
    onSuccess: (profile: EmployeeSensitiveProfile): void => {
      setSensitive(profile);
      setCitizenCountry(profile.citizen_id_country_code ?? "VN");
      setCitizenId(profile.citizen_id ?? "");
      setDraft((current: EmployeeUpsertRequest | null): EmployeeUpsertRequest | null =>
        current ? { ...current, expected_version: profile.version } : null,
      );
      void queryClient.invalidateQueries({ queryKey: peopleQueryKeys.employees });
      setFeedback(profile.citizen_id ? "Đã cập nhật số giấy tờ định danh." : "Đã xóa số giấy tờ định danh.");
    },
  });

  const openEditor = (employee: Employee): void => {
    setEditingEmployee(employee);
    setDraft(employeeDraft(employee));
    setSensitive(null);
    setCitizenCountry(employee.citizen_id_country_code ?? "VN");
    setCitizenId("");
    setSensitiveError(null);
    setFeedback(null);
  };

  const closeEditor = (): void => {
    setEditingEmployee(null);
    setDraft(null);
    setSensitive(null);
    setCitizenId("");
    setSensitiveError(null);
  };

  const revealCitizenId = async (): Promise<void> => {
    if (!editingEmployee) return;
    setSensitiveLoading(true);
    setSensitiveError(null);
    try {
      const profile: EmployeeSensitiveProfile = await getEmployeeCitizenId(editingEmployee.id);
      setSensitive(profile);
      setCitizenCountry(profile.citizen_id_country_code ?? "VN");
      setCitizenId(profile.citizen_id ?? "");
      setDraft((current: EmployeeUpsertRequest | null): EmployeeUpsertRequest | null =>
        current ? { ...current, expected_version: profile.version } : null,
      );
    } catch (error: unknown) {
      setSensitiveError(friendlyApiError(error, "Không thể đọc giấy tờ định danh."));
    } finally {
      setSensitiveLoading(false);
    }
  };

  const submitProfile = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    if (!editingEmployee || !draft) return;
    saveMutation.mutate({
      employeeId: editingEmployee.id,
      payload: {
        ...draft,
        employee_code: draft.employee_code.trim().toLocaleLowerCase("en-US"),
        display_name: draft.display_name.trim(),
        legal_first_name: optionalValue(draft.legal_first_name ?? ""),
        legal_middle_name: optionalValue(draft.legal_middle_name ?? ""),
        legal_last_name: optionalValue(draft.legal_last_name ?? ""),
        personal_phone_e164: optionalValue(draft.personal_phone_e164 ?? ""),
        termination_date: draft.status === "terminated" ? draft.termination_date : null,
      },
    });
  };

  const saveCitizenId = (): void => {
    if (!editingEmployee || !draft?.expected_version) return;
    const normalizedCitizenId: string | null = optionalValue(citizenId);
    citizenMutation.mutate({
      employeeId: editingEmployee.id,
      payload: {
        citizen_id_country_code: normalizedCitizenId ? citizenCountry.trim().toUpperCase() : null,
        citizen_id: normalizedCitizenId,
        expected_version: draft.expected_version,
      },
    });
  };

  if (!canRead) {
    return <section className="panel p-8 text-center"><p className="font-bold text-slate-900">Bạn không có quyền xem hồ sơ nhân sự.</p></section>;
  }

  return (
    <section className="space-y-5">
      <div className="panel flex flex-col gap-4 p-5 sm:flex-row sm:items-center sm:justify-between">
        <label className="relative block w-full max-w-xl">
          <Search className="absolute left-3 top-3 size-5 text-slate-400" />
          <input className="min-h-11 w-full rounded-xl border-slate-300 pl-10" onChange={(event): void => setSearch(event.target.value)} placeholder="Tìm theo tên, mã hoặc điện thoại" type="search" value={search} />
        </label>
        <button aria-label="Tải lại" className="action-secondary shrink-0" onClick={(): void => { void employeesQuery.refetch(); }} type="button">
          <RefreshCw className={`size-4 ${employeesQuery.isFetching ? "animate-spin" : ""}`} />
          Tải lại
        </button>
      </div>

      {feedback ? <div className="rounded-xl bg-emerald-50 px-4 py-3 text-sm font-medium text-emerald-800">{feedback}</div> : null}
      {employeesQuery.error ? <div className="rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(employeesQuery.error, "Không thể tải hồ sơ nhân sự.")}</div> : null}

      <div className="panel overflow-hidden">
        <div className="border-b border-slate-200 px-5 py-4">
          <h2 className="font-black text-slate-950">Nhân viên trong chi nhánh đang chọn</h2>
          <p className="mt-1 text-sm text-slate-500">{visibleEmployees.length} nhân viên · chủ doanh nghiệp không thuộc danh sách nhân viên</p>
        </div>
        {employeesQuery.isPending ? (
          <div className="grid min-h-48 place-items-center"><LoaderCircle className="size-6 animate-spin text-blue-600" /></div>
        ) : visibleEmployees.length === 0 ? (
          <div className="p-10 text-center text-sm text-slate-500">Chưa có nhân viên phù hợp.</div>
        ) : (
          <div className="divide-y divide-slate-100">
            {visibleEmployees.map((employee: Employee): React.JSX.Element => (
              <article className="flex flex-col gap-3 px-5 py-4 sm:flex-row sm:items-center" key={employee.id}>
                <div className="grid size-11 shrink-0 place-items-center rounded-xl bg-blue-50 text-blue-700"><UserRound className="size-5" /></div>
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="font-bold text-slate-950">{employee.display_name}</h3>
                    <span className="rounded-full bg-slate-100 px-2 py-1 text-xs font-bold text-slate-600">{employee.employee_code}</span>
                    <span className={`rounded-full px-2 py-1 text-xs font-bold ${employee.profile_complete ? "bg-emerald-50 text-emerald-700" : "bg-amber-50 text-amber-700"}`}>{employee.profile_complete ? "Đủ họ tên pháp lý" : "Thiếu hồ sơ"}</span>
                  </div>
                  <p className="mt-1 text-sm text-slate-600">{[employee.legal_last_name, employee.legal_middle_name, employee.legal_first_name].filter(Boolean).join(" ") || "Chưa cập nhật họ tên pháp lý"}</p>
                  <p className="mt-1 text-xs text-slate-400">{genderLabel(employee.gender)} · {employee.personal_phone_e164 ?? "Chưa có điện thoại cá nhân"} · {statusLabel(employee.status)}{employee.citizen_id_last4 ? ` · Giấy tờ ••••${employee.citizen_id_last4}` : ""}</p>
                </div>
                {canManage ? <button className="action-secondary" onClick={(): void => openEditor(employee)} type="button"><Pencil className="size-4" />Chỉnh sửa</button> : null}
              </article>
            ))}
          </div>
        )}
      </div>

      {editingEmployee && draft ? (
        <div className="fixed inset-0 z-50 overflow-y-auto bg-slate-950/50 p-4">
          <div className="mx-auto my-6 w-full max-w-3xl rounded-2xl bg-white p-6 shadow-2xl">
            <div className="flex items-start justify-between gap-4">
              <div><h2 className="text-xl font-black text-slate-950">Hồ sơ {editingEmployee.display_name}</h2><p className="mt-1 text-sm text-slate-500">Thông tin nhân thân thuộc HR; tài khoản đăng nhập vẫn do hệ thống Auth quản lý.</p></div>
              <button aria-label="Đóng" className="grid size-9 place-items-center rounded-lg hover:bg-slate-100" onClick={closeEditor} type="button"><X className="size-5" /></button>
            </div>
            <form className="mt-6" onSubmit={submitProfile}>
              <div className="grid gap-4 sm:grid-cols-2">
                <label className="text-sm font-semibold text-slate-700">Tên hiển thị<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={200} onChange={(event): void => setDraft({ ...draft, display_name: event.target.value })} required value={draft.display_name} /></label>
                <label className="text-sm font-semibold text-slate-700">Mã nhân viên<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={63} onChange={(event): void => setDraft({ ...draft, employee_code: event.target.value })} required value={draft.employee_code} /></label>
                <label className="text-sm font-semibold text-slate-700">Họ<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={100} onChange={(event): void => setDraft({ ...draft, legal_last_name: event.target.value })} value={draft.legal_last_name ?? ""} /></label>
                <label className="text-sm font-semibold text-slate-700">Tên đệm<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={100} onChange={(event): void => setDraft({ ...draft, legal_middle_name: event.target.value })} value={draft.legal_middle_name ?? ""} /></label>
                <label className="text-sm font-semibold text-slate-700">Tên<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={100} onChange={(event): void => setDraft({ ...draft, legal_first_name: event.target.value })} value={draft.legal_first_name ?? ""} /></label>
                <label className="text-sm font-semibold text-slate-700">Giới tính<select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setDraft({ ...draft, gender: (event.target.value || null) as Gender | null })} value={draft.gender ?? ""}><option value="">Chưa cập nhật</option><option value="female">Nữ</option><option value="male">Nam</option><option value="other">Khác</option><option value="unspecified">Không công bố</option></select></label>
                <label className="text-sm font-semibold text-slate-700">Điện thoại cá nhân (E.164)<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={16} onChange={(event): void => setDraft({ ...draft, personal_phone_e164: event.target.value })} placeholder="+84901234567" type="tel" value={draft.personal_phone_e164 ?? ""} /></label>
                <label className="text-sm font-semibold text-slate-700">Ngày vào làm<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setDraft({ ...draft, hire_date: event.target.value })} required type="date" value={draft.hire_date} /></label>
                <label className="text-sm font-semibold text-slate-700">Trạng thái<select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setDraft({ ...draft, status: event.target.value as EmployeeStatus })} value={draft.status}><option value="active">Đang làm việc</option><option value="on_leave">Đang nghỉ</option><option value="terminated">Đã nghỉ việc</option></select></label>
                {draft.status === "terminated" ? <label className="text-sm font-semibold text-slate-700">Ngày nghỉ việc<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" min={draft.hire_date} onChange={(event): void => setDraft({ ...draft, termination_date: event.target.value })} required type="date" value={draft.termination_date ?? ""} /></label> : null}
              </div>
              {saveMutation.error ? <p className="mt-4 rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(saveMutation.error, "Không thể lưu hồ sơ. Dữ liệu có thể vừa được người khác cập nhật.")}</p> : null}
              <div className="mt-6 flex justify-end"><button className="action-primary" disabled={saveMutation.isPending} type="submit">{saveMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : null}Lưu hồ sơ</button></div>
            </form>

            {canReadSensitive ? (
              <section className="mt-7 rounded-2xl border border-amber-200 bg-amber-50/50 p-5">
                <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  <div><h3 className="flex items-center gap-2 font-black text-slate-950"><ShieldCheck className="size-5 text-amber-700" />Giấy tờ định danh nhạy cảm</h3><p className="mt-1 text-sm text-slate-600">Chỉ lần xem này mới giải mã số đầy đủ; danh sách chỉ hiển thị 4 ký tự cuối.</p></div>
                  {!sensitive ? <button className="action-secondary" disabled={sensitiveLoading} onClick={(): void => { void revealCitizenId(); }} type="button">{sensitiveLoading ? <LoaderCircle className="size-4 animate-spin" /> : <Eye className="size-4" />}Xem giấy tờ</button> : null}
                </div>
                {sensitiveError ? <p className="mt-4 rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{sensitiveError}</p> : null}
                {sensitive ? (
                  <div className="mt-4 grid gap-4 sm:grid-cols-[8rem_1fr]">
                    <label className="text-sm font-semibold text-slate-700">Quốc gia<input className="mt-2 min-h-11 w-full rounded-xl border-slate-300 uppercase" disabled={!canManageSensitive} maxLength={2} onChange={(event): void => setCitizenCountry(event.target.value)} value={citizenCountry} /></label>
                    <label className="text-sm font-semibold text-slate-700">Số giấy tờ<input autoComplete="off" className="mt-2 min-h-11 w-full rounded-xl border-slate-300" disabled={!canManageSensitive} maxLength={40} onChange={(event): void => setCitizenId(event.target.value)} value={citizenId} /></label>
                    {canManageSensitive ? <div className="sm:col-span-2 flex justify-end"><button className="action-primary" disabled={citizenMutation.isPending} onClick={saveCitizenId} type="button">{citizenMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : null}{citizenId.trim() ? "Lưu giấy tờ" : "Xóa giấy tờ"}</button></div> : null}
                    {citizenMutation.error ? <p className="sm:col-span-2 rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(citizenMutation.error, "Không thể cập nhật giấy tờ. Dữ liệu có thể vừa được người khác cập nhật.")}</p> : null}
                  </div>
                ) : null}
              </section>
            ) : null}
          </div>
        </div>
      ) : null}
    </section>
  );
}
