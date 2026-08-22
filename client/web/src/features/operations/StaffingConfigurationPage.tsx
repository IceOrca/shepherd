import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { BadgeDollarSign, LoaderCircle, Plus, ShieldCheck } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import type {
  Customer,
  CustomerFacility,
  Employee,
  JobPosition,
  PermissionCode,
  StaffingEligibility,
  StaffingEligibilityCreateRequest,
  StaffingRate,
  StaffingRateCreateRequest,
  StaffingRateKind,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { useAuth } from "../auth/AuthProvider";
import {
  createStaffingEligibility,
  createStaffingRate,
  listCustomerFacilities,
  listCustomers,
  listEmployees,
  listJobs,
  listStaffingEligibilities,
  listStaffingRates,
  operationsQueryKeys,
} from "./api";

interface RateDraft {
  rateKind: StaffingRateKind;
  code: string;
  name: string;
  customerId: string;
  customerFacilityId: string;
  employeeId: string;
  jobId: string;
  currency: string;
  hourlyRate: string;
  priority: string;
  effectiveFrom: string;
  effectiveTo: string;
}

interface EligibilityDraft {
  employeeId: string;
  jobId: string;
  effectiveFrom: string;
  effectiveTo: string;
  notes: string;
}

const today: string = new Date().toISOString().slice(0, 10);

const emptyRateDraft: RateDraft = {
  rateKind: "customer_bill",
  code: "",
  name: "",
  customerId: "",
  customerFacilityId: "",
  employeeId: "",
  jobId: "",
  currency: "VND",
  hourlyRate: "",
  priority: "0",
  effectiveFrom: today,
  effectiveTo: "",
};

const emptyEligibilityDraft: EligibilityDraft = {
  employeeId: "",
  jobId: "",
  effectiveFrom: today,
  effectiveTo: "",
  notes: "",
};

function rateKindLabel(rateKind: StaffingRateKind): string {
  return rateKind === "customer_bill" ? "Giá thu khách hàng" : "Tiền công nhân viên";
}

export function StaffingConfigurationPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const queryClient: ReturnType<typeof useQueryClient> = useQueryClient();
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canReadRates: boolean = permissions.includes("business.staffing_rates.read");
  const canManageRates: boolean = permissions.includes("business.staffing_rates.manage");
  const canReadEligibility: boolean = permissions.includes("business.staffing_eligibility.read");
  const canManageEligibility: boolean = permissions.includes("business.staffing_eligibility.manage");
  const [rateDraft, setRateDraft] = useState<RateDraft>(emptyRateDraft);
  const [eligibilityDraft, setEligibilityDraft] = useState<EligibilityDraft>(emptyEligibilityDraft);
  const [feedback, setFeedback] = useState<string | null>(null);

  const ratesQuery: UseQueryResult<StaffingRate[], Error> = useQuery({
    queryKey: operationsQueryKeys.staffingRates,
    queryFn: listStaffingRates,
    enabled: canReadRates,
  });
  const eligibilityQuery: UseQueryResult<StaffingEligibility[], Error> = useQuery({
    queryKey: operationsQueryKeys.staffingEligibilities,
    queryFn: listStaffingEligibilities,
    enabled: canReadEligibility,
  });
  const customersQuery: UseQueryResult<Customer[], Error> = useQuery({
    queryKey: operationsQueryKeys.customers,
    queryFn: listCustomers,
    enabled: canReadRates,
  });
  const jobsQuery: UseQueryResult<JobPosition[], Error> = useQuery({
    queryKey: operationsQueryKeys.jobs,
    queryFn: listJobs,
    enabled: canReadRates || canReadEligibility,
  });
  const employeesQuery: UseQueryResult<Employee[], Error> = useQuery({
    queryKey: operationsQueryKeys.employees,
    queryFn: listEmployees,
    enabled: canReadRates || canReadEligibility,
  });
  const facilitiesQuery: UseQueryResult<CustomerFacility[], Error> = useQuery({
    queryKey: rateDraft.customerId
      ? operationsQueryKeys.facilities(rateDraft.customerId)
      : ["operations", "customers", "none", "facilities"],
    queryFn: (): Promise<CustomerFacility[]> => listCustomerFacilities(rateDraft.customerId),
    enabled: canReadRates && Boolean(rateDraft.customerId),
  });

  const customerNames: Map<string, string> = useMemo(
    (): Map<string, string> =>
      new Map((customersQuery.data ?? []).map((customer: Customer): [string, string] => [customer.id, customer.name])),
    [customersQuery.data],
  );
  const employeeNames: Map<string, string> = useMemo(
    (): Map<string, string> =>
      new Map((employeesQuery.data ?? []).map((employee: Employee): [string, string] => [employee.id, employee.display_name])),
    [employeesQuery.data],
  );
  const jobNames: Map<string, string> = useMemo(
    (): Map<string, string> =>
      new Map((jobsQuery.data ?? []).map((job: JobPosition): [string, string] => [job.id, job.name])),
    [jobsQuery.data],
  );

  const rateMutation: UseMutationResult<StaffingRate, Error, StaffingRateCreateRequest> = useMutation({
    mutationFn: createStaffingRate,
    onSuccess: (rate: StaffingRate): void => {
      void queryClient.invalidateQueries({ queryKey: operationsQueryKeys.staffingRates });
      setRateDraft(emptyRateDraft);
      setFeedback("Đã tạo " + rateKindLabel(rate.rate_kind).toLocaleLowerCase("vi") + " " + rate.name + ".");
    },
  });
  const eligibilityMutation: UseMutationResult<
    StaffingEligibility,
    Error,
    StaffingEligibilityCreateRequest
  > = useMutation({
    mutationFn: createStaffingEligibility,
    onSuccess: (): void => {
      void queryClient.invalidateQueries({ queryKey: operationsQueryKeys.staffingEligibilities });
      setEligibilityDraft(emptyEligibilityDraft);
      setFeedback("Đã thêm năng lực làm dịch vụ theo thời hạn.");
    },
  });

  const submitRate: (event: FormEvent<HTMLFormElement>) => void = (
    event: FormEvent<HTMLFormElement>,
  ): void => {
    event.preventDefault();
    setFeedback(null);
    rateMutation.mutate({
      rate_kind: rateDraft.rateKind,
      code: rateDraft.code.trim().toLocaleLowerCase("en-US"),
      name: rateDraft.name.trim(),
      customer_id: rateDraft.customerId || null,
      customer_facility_id: rateDraft.customerFacilityId || null,
      employee_id: rateDraft.employeeId || null,
      job_id: rateDraft.jobId,
      currency: rateDraft.currency.trim().toLocaleUpperCase("en-US"),
      hourly_rate: rateDraft.hourlyRate.trim(),
      priority: Number(rateDraft.priority),
      effective_from: rateDraft.effectiveFrom,
      effective_to: rateDraft.effectiveTo || null,
      is_active: true,
    });
  };

  const submitEligibility: (event: FormEvent<HTMLFormElement>) => void = (
    event: FormEvent<HTMLFormElement>,
  ): void => {
    event.preventDefault();
    setFeedback(null);
    eligibilityMutation.mutate({
      employee_id: eligibilityDraft.employeeId,
      job_id: eligibilityDraft.jobId,
      effective_from: eligibilityDraft.effectiveFrom,
      effective_to: eligibilityDraft.effectiveTo || null,
      notes: eligibilityDraft.notes.trim() || null,
    });
  };

  if (!canReadRates && !canReadEligibility) {
    return <section className="panel p-8 text-center text-sm text-slate-500">Bạn chưa có quyền xem cấu hình nhân sự dịch vụ.</section>;
  }

  const loading: boolean =
    ratesQuery.isPending || eligibilityQuery.isPending || jobsQuery.isPending || employeesQuery.isPending;

  if (loading) {
    return <section className="panel p-8 text-center text-sm text-slate-500"><LoaderCircle className="mr-2 inline size-4 animate-spin" />Đang tải cấu hình...</section>;
  }

  const error: Error | null =
    ratesQuery.error ?? eligibilityQuery.error ?? jobsQuery.error ?? employeesQuery.error ?? customersQuery.error ?? null;

  return (
    <div className="space-y-5">
      {feedback ? <div className="rounded-xl bg-emerald-50 px-4 py-3 text-sm font-medium text-emerald-800">{feedback}</div> : null}
      {error ? <div className="rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(error, "Không thể tải cấu hình nghiệp vụ.")}</div> : null}
      {rateMutation.error ? <div className="rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(rateMutation.error, "Không thể tạo mức giá. Kiểm tra phạm vi và thời hạn bị trùng.")}</div> : null}
      {eligibilityMutation.error ? <div className="rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(eligibilityMutation.error, "Không thể tạo năng lực làm dịch vụ.")}</div> : null}

      <div className="grid gap-5 xl:grid-cols-2">
        <section className="panel p-5 sm:p-6">
          <div className="flex items-center gap-2"><BadgeDollarSign className="size-5 text-blue-600" /><h2 className="font-black text-slate-950">Giá thu và tiền công</h2></div>
          <p className="mt-1 text-sm text-slate-500">Hai loại giá độc lập; tiền công có thể thay đổi theo nhân viên, khách hàng, cơ sở và ngày hiệu lực.</p>
          {canManageRates ? (
            <form className="mt-5 grid gap-3 sm:grid-cols-2" onSubmit={submitRate}>
              <label className="text-sm font-semibold text-slate-700">Loại giá<select className="mt-1.5 w-full rounded-xl border-slate-300" value={rateDraft.rateKind} onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setRateDraft({ ...rateDraft, rateKind: event.target.value as StaffingRateKind })}><option value="customer_bill">Giá thu khách hàng</option><option value="worker_pay">Tiền công nhân viên</option></select></label>
              <label className="text-sm font-semibold text-slate-700">Mã<input className="mt-1.5 w-full rounded-xl border-slate-300" required value={rateDraft.code} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setRateDraft({ ...rateDraft, code: event.target.value })} /></label>
              <label className="text-sm font-semibold text-slate-700 sm:col-span-2">Tên<input className="mt-1.5 w-full rounded-xl border-slate-300" required value={rateDraft.name} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setRateDraft({ ...rateDraft, name: event.target.value })} /></label>
              <label className="text-sm font-semibold text-slate-700">Khách hàng<select className="mt-1.5 w-full rounded-xl border-slate-300" required={rateDraft.rateKind === "customer_bill"} value={rateDraft.customerId} onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setRateDraft({ ...rateDraft, customerId: event.target.value, customerFacilityId: "" })}><option value="">{rateDraft.rateKind === "worker_pay" ? "Mặc định mọi khách hàng" : "Chọn khách hàng"}</option>{(customersQuery.data ?? []).map((customer: Customer) => <option key={customer.id} value={customer.id}>{customer.name}</option>)}</select></label>
              <label className="text-sm font-semibold text-slate-700">Cơ sở<select className="mt-1.5 w-full rounded-xl border-slate-300" disabled={!rateDraft.customerId} value={rateDraft.customerFacilityId} onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setRateDraft({ ...rateDraft, customerFacilityId: event.target.value })}><option value="">Mọi cơ sở</option>{(facilitiesQuery.data ?? []).map((facility: CustomerFacility) => <option key={facility.id} value={facility.id}>{facility.name}</option>)}</select></label>
              <label className="text-sm font-semibold text-slate-700">Nhân viên<select className="mt-1.5 w-full rounded-xl border-slate-300" value={rateDraft.employeeId} onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setRateDraft({ ...rateDraft, employeeId: event.target.value })}><option value="">Mọi nhân viên phù hợp</option>{(employeesQuery.data ?? []).filter((employee: Employee): boolean => employee.status === "active").map((employee: Employee) => <option key={employee.id} value={employee.id}>{employee.display_name}</option>)}</select></label>
              <label className="text-sm font-semibold text-slate-700">Dịch vụ / công việc<select className="mt-1.5 w-full rounded-xl border-slate-300" required value={rateDraft.jobId} onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setRateDraft({ ...rateDraft, jobId: event.target.value })}><option value="">Chọn công việc</option>{(jobsQuery.data ?? []).map((job: JobPosition) => <option key={job.id} value={job.id}>{job.name}</option>)}</select></label>
              <label className="text-sm font-semibold text-slate-700">Đơn giá / giờ<input className="mt-1.5 w-full rounded-xl border-slate-300" inputMode="decimal" required value={rateDraft.hourlyRate} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setRateDraft({ ...rateDraft, hourlyRate: event.target.value })} /></label>
              <label className="text-sm font-semibold text-slate-700">Tiền tệ<input className="mt-1.5 w-full rounded-xl border-slate-300" maxLength={3} required value={rateDraft.currency} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setRateDraft({ ...rateDraft, currency: event.target.value })} /></label>
              <label className="text-sm font-semibold text-slate-700">Hiệu lực từ<input className="mt-1.5 w-full rounded-xl border-slate-300" required type="date" value={rateDraft.effectiveFrom} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setRateDraft({ ...rateDraft, effectiveFrom: event.target.value })} /></label>
              <label className="text-sm font-semibold text-slate-700">Hiệu lực đến<input className="mt-1.5 w-full rounded-xl border-slate-300" type="date" value={rateDraft.effectiveTo} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setRateDraft({ ...rateDraft, effectiveTo: event.target.value })} /></label>
              <label className="text-sm font-semibold text-slate-700">Ưu tiên<input className="mt-1.5 w-full rounded-xl border-slate-300" required type="number" value={rateDraft.priority} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setRateDraft({ ...rateDraft, priority: event.target.value })} /></label>
              <button className="action-primary self-end" disabled={rateMutation.isPending} type="submit"><Plus className="size-4" />Thêm mức giá</button>
            </form>
          ) : null}
        </section>

        <section className="panel p-5 sm:p-6">
          <div className="flex items-center gap-2"><ShieldCheck className="size-5 text-violet-600" /><h2 className="font-black text-slate-950">Năng lực làm dịch vụ</h2></div>
          <p className="mt-1 text-sm text-slate-500">Độc lập với chức danh HR chính; một nhân viên có thể làm nhiều loại công việc khách hàng.</p>
          {canManageEligibility ? (
            <form className="mt-5 grid gap-3 sm:grid-cols-2" onSubmit={submitEligibility}>
              <label className="text-sm font-semibold text-slate-700">Nhân viên<select className="mt-1.5 w-full rounded-xl border-slate-300" required value={eligibilityDraft.employeeId} onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setEligibilityDraft({ ...eligibilityDraft, employeeId: event.target.value })}><option value="">Chọn nhân viên</option>{(employeesQuery.data ?? []).filter((employee: Employee): boolean => employee.status === "active").map((employee: Employee) => <option key={employee.id} value={employee.id}>{employee.display_name}</option>)}</select></label>
              <label className="text-sm font-semibold text-slate-700">Dịch vụ / công việc<select className="mt-1.5 w-full rounded-xl border-slate-300" required value={eligibilityDraft.jobId} onChange={(event: React.ChangeEvent<HTMLSelectElement>): void => setEligibilityDraft({ ...eligibilityDraft, jobId: event.target.value })}><option value="">Chọn công việc</option>{(jobsQuery.data ?? []).map((job: JobPosition) => <option key={job.id} value={job.id}>{job.name}</option>)}</select></label>
              <label className="text-sm font-semibold text-slate-700">Hiệu lực từ<input className="mt-1.5 w-full rounded-xl border-slate-300" required type="date" value={eligibilityDraft.effectiveFrom} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setEligibilityDraft({ ...eligibilityDraft, effectiveFrom: event.target.value })} /></label>
              <label className="text-sm font-semibold text-slate-700">Hiệu lực đến<input className="mt-1.5 w-full rounded-xl border-slate-300" type="date" value={eligibilityDraft.effectiveTo} onChange={(event: React.ChangeEvent<HTMLInputElement>): void => setEligibilityDraft({ ...eligibilityDraft, effectiveTo: event.target.value })} /></label>
              <label className="text-sm font-semibold text-slate-700 sm:col-span-2">Ghi chú<textarea className="mt-1.5 w-full rounded-xl border-slate-300" rows={2} value={eligibilityDraft.notes} onChange={(event: React.ChangeEvent<HTMLTextAreaElement>): void => setEligibilityDraft({ ...eligibilityDraft, notes: event.target.value })} /></label>
              <button className="action-primary sm:col-span-2" disabled={eligibilityMutation.isPending} type="submit"><Plus className="size-4" />Thêm năng lực</button>
            </form>
          ) : null}
        </section>
      </div>

      <div className="grid gap-5 xl:grid-cols-2">
        <section className="panel overflow-hidden">
          <div className="border-b border-slate-200 px-5 py-4 font-bold text-slate-950">Mức giá đang cấu hình</div>
          <div className="divide-y divide-slate-100">
            {(ratesQuery.data ?? []).map((rate: StaffingRate) => <div className="px-5 py-4" key={rate.id}><div className="flex items-start justify-between gap-3"><div><p className="font-bold text-slate-900">{rate.name}</p><p className="mt-1 text-xs text-slate-500">{customerNames.get(rate.customer_id ?? "") ?? "Mọi khách hàng"} · {employeeNames.get(rate.employee_id ?? "") ?? "Mọi nhân viên"} · {jobNames.get(rate.job_id) ?? rate.job_id}</p></div><span className="rounded-full bg-blue-50 px-2.5 py-1 text-xs font-bold text-blue-700">{rateKindLabel(rate.rate_kind)}</span></div><p className="mt-2 text-sm font-semibold text-slate-700">{rate.hourly_rate} {rate.currency}/giờ · từ {rate.effective_from}</p></div>)}
            {(ratesQuery.data ?? []).length === 0 ? <p className="p-6 text-center text-sm text-slate-500">Chưa có mức giá.</p> : null}
          </div>
        </section>
        <section className="panel overflow-hidden">
          <div className="border-b border-slate-200 px-5 py-4 font-bold text-slate-950">Năng lực nhân viên</div>
          <div className="divide-y divide-slate-100">
            {(eligibilityQuery.data ?? []).map((eligibility: StaffingEligibility) => <div className="px-5 py-4" key={eligibility.id}><p className="font-bold text-slate-900">{employeeNames.get(eligibility.employee_id) ?? eligibility.employee_id}</p><p className="mt-1 text-sm text-slate-600">{jobNames.get(eligibility.job_id) ?? eligibility.job_id}</p><p className="mt-1 text-xs text-slate-500">Từ {eligibility.effective_from}{eligibility.effective_to ? " đến " + eligibility.effective_to : ""}</p></div>)}
            {(eligibilityQuery.data ?? []).length === 0 ? <p className="p-6 text-center text-sm text-slate-500">Chưa có năng lực dịch vụ.</p> : null}
          </div>
        </section>
      </div>
    </div>
  );
}
