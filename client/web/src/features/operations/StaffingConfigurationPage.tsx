import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  BadgeDollarSign,
  CalendarDays,
  LoaderCircle,
  Pencil,
  Search,
  UsersRound,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import type {
  Customer,
  PermissionCode,
  StaffingPriceSet,
  StaffingPriceSetRequest,
  StaffingRate,
  StaffingRateKind,
  StaffingStaff,
  StaffingStaffPageResponse,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { CursorPagination } from "../../shared/components/CursorPagination";
import { useAuth } from "../auth/AuthProvider";
import {
  listCustomers,
  listStaffingRates,
  listStaffingStaff,
  operationsQueryKeys,
  setStaffingPrices,
} from "./api";

interface PriceDraft {
  employeeId: string | null;
  customerHourlyRate: string;
  workerHourlyRate: string;
  currency: string;
  effectiveFrom: string;
}

interface ResolvedPrice {
  rate: StaffingRate | null;
  inherited: boolean;
}

interface PriceRow {
  employeeId: string | null;
  employeeCode: string | null;
  displayName: string;
  customerBill: ResolvedPrice;
  workerPay: ResolvedPrice;
}

function localIsoDate(): string {
  const now: Date = new Date();
  const year: string = String(now.getFullYear());
  const month: string = String(now.getMonth() + 1).padStart(2, "0");
  const day: string = String(now.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

const today: string = localIsoDate();

function formatDecimal(value: string): string {
  const [integerPart, decimalPart = ""]: string[] = value.split(".");
  const grouped: string = integerPart.replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  const fraction: string = decimalPart.replace(/0+$/, "");
  return fraction ? `${grouped},${fraction}` : grouped;
}

function formatDate(value: string): string {
  const [year, month, day]: string[] = value.split("-");
  return `${day}/${month}/${year}`;
}

function compareRates(left: StaffingRate, right: StaffingRate, rateKind: StaffingRateKind): number {
  const employeeSpecificity: number = Number(right.employee_id !== null) - Number(left.employee_id !== null);
  if (employeeSpecificity !== 0) return employeeSpecificity;
  if (rateKind === "worker_pay") {
    const customerSpecificity: number = Number(right.customer_id !== null) - Number(left.customer_id !== null);
    if (customerSpecificity !== 0) return customerSpecificity;
  }
  if (left.priority !== right.priority) return right.priority - left.priority;
  const effectiveOrder: number = right.effective_from.localeCompare(left.effective_from);
  return effectiveOrder !== 0 ? effectiveOrder : left.id.localeCompare(right.id);
}

function resolveRate(
  rates: StaffingRate[],
  rateKind: StaffingRateKind,
  customerId: string,
  employeeId: string | null,
  workDate: string,
): ResolvedPrice {
  const candidates: StaffingRate[] = rates
    .filter((rate: StaffingRate): boolean => {
      if (!rate.is_active || rate.rate_kind !== rateKind) return false;
      if (rate.effective_from > workDate || (rate.effective_to !== null && rate.effective_to < workDate)) return false;
      if (rateKind === "customer_bill" && rate.customer_id !== customerId) return false;
      if (rateKind === "worker_pay" && rate.customer_id !== null && rate.customer_id !== customerId) return false;
      if (employeeId === null) return rate.employee_id === null;
      return rate.employee_id === null || rate.employee_id === employeeId;
    })
    .sort((left: StaffingRate, right: StaffingRate): number => compareRates(left, right, rateKind));
  const rate: StaffingRate | null = candidates[0] ?? null;
  return { rate, inherited: employeeId !== null && rate?.employee_id === null };
}

function RateCell({ price }: { price: ResolvedPrice }): React.JSX.Element {
  if (price.rate === null) {
    return <span className="font-semibold text-amber-700">Chưa thiết lập</span>;
  }
  return (
    <div>
      <p className="text-base font-black tabular-nums text-slate-950">
        {formatDecimal(price.rate.hourly_rate)} <span className="text-xs font-bold text-slate-500">{price.rate.currency}/giờ</span>
      </p>
      {price.inherited ? <p className="mt-1 text-xs font-semibold text-blue-600">Theo mức mặc định</p> : null}
    </div>
  );
}

function effectiveLabel(row: PriceRow): string {
  const billDate: string | null = row.customerBill.rate?.effective_from ?? null;
  const payDate: string | null = row.workerPay.rate?.effective_from ?? null;
  if (billDate === null && payDate === null) return "—";
  if (billDate === payDate && billDate !== null) return formatDate(billDate);
  return `Thu: ${billDate ? formatDate(billDate) : "—"} · Công: ${payDate ? formatDate(payDate) : "—"}`;
}

export function StaffingConfigurationPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const queryClient: ReturnType<typeof useQueryClient> = useQueryClient();
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canRead: boolean = permissions.includes("business.staffing_rates.read");
  const canManage: boolean = permissions.includes("business.staffing_rates.manage");
  const [selectedCustomerId, setSelectedCustomerId] = useState<string>("");
  const [viewDate, setViewDate] = useState<string>(today);
  const [search, setSearch] = useState<string>("");
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [draft, setDraft] = useState<PriceDraft | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);

  const ratesQuery: UseQueryResult<StaffingRate[], Error> = useQuery({
    queryKey: [...operationsQueryKeys.staffingRates, "customer", selectedCustomerId],
    queryFn: (): Promise<StaffingRate[]> => listStaffingRates(selectedCustomerId),
    enabled: canRead && selectedCustomerId !== "",
  });
  const staffQuery = useInfiniteQuery({
    queryKey: [...operationsQueryKeys.staffingStaff, "search", search.trim()],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }: { pageParam: string | null }): Promise<StaffingStaffPageResponse> =>
      listStaffingStaff(pageParam, search),
    getNextPageParam: (lastPage: StaffingStaffPageResponse): string | undefined => lastPage.next_cursor ?? undefined,
    enabled: canRead,
  });
  const customersQuery: UseQueryResult<Customer[], Error> = useQuery({
    queryKey: operationsQueryKeys.customers,
    queryFn: listCustomers,
    enabled: canRead,
  });

  const activeCustomers: Customer[] = useMemo(
    (): Customer[] => (customersQuery.data ?? []).filter((customer: Customer): boolean => customer.status === "active"),
    [customersQuery.data],
  );

  useEffect((): void => {
    if (selectedCustomerId === "" && activeCustomers.length > 0) {
      setSelectedCustomerId(activeCustomers[0]?.id ?? "");
    }
  }, [activeCustomers, selectedCustomerId]);

  const loadedStaffPages: StaffingStaffPageResponse[] = staffQuery.data?.pages ?? [];
  const pageStaff: StaffingStaff[] = loadedStaffPages[currentPage - 1]?.items ?? [];
  const hasNextPage: boolean = currentPage < loadedStaffPages.length || staffQuery.hasNextPage;

  useEffect((): void => setCurrentPage(1), [search, selectedCustomerId]);

  const changePage = (nextPage: number): void => {
    if (nextPage < 1) return;
    if (nextPage <= loadedStaffPages.length) {
      setCurrentPage(nextPage);
      return;
    }
    if (nextPage === loadedStaffPages.length + 1 && staffQuery.hasNextPage) {
      void staffQuery.fetchNextPage().then((result): void => {
        if ((result.data?.pages.length ?? 0) >= nextPage) setCurrentPage(nextPage);
      });
    }
  };

  const rows: PriceRow[] = useMemo((): PriceRow[] => {
    if (selectedCustomerId === "") return [];
    const rates: StaffingRate[] = ratesQuery.data ?? [];
    const makeRow = (staff: StaffingStaff | null): PriceRow => {
      const employeeId: string | null = staff?.employee_id ?? null;
      return {
        employeeId,
        employeeCode: staff?.employee_code ?? null,
        displayName: staff?.display_name ?? "Toàn bộ nhân viên Staff",
        customerBill: resolveRate(rates, "customer_bill", selectedCustomerId, employeeId, viewDate),
        workerPay: resolveRate(rates, "worker_pay", selectedCustomerId, employeeId, viewDate),
      };
    };
    const defaultRows: PriceRow[] = currentPage === 1 ? [makeRow(null)] : [];
    return [...defaultRows, ...pageStaff.map((staff: StaffingStaff): PriceRow => makeRow(staff))];
  }, [currentPage, pageStaff, ratesQuery.data, selectedCustomerId, viewDate]);

  const visibleRows: PriceRow[] = rows;

  const priceMutation: UseMutationResult<StaffingPriceSet, Error, StaffingPriceSetRequest> = useMutation({
    mutationFn: setStaffingPrices,
    onSuccess: (): void => {
      void queryClient.invalidateQueries({ queryKey: operationsQueryKeys.staffingRates });
      setDraft(null);
      setFeedback("Đã lưu mức giá mới. Các mức cũ vẫn được giữ lại trong lịch sử.");
    },
  });

  const selectedCustomer: Customer | null =
    activeCustomers.find((customer: Customer): boolean => customer.id === selectedCustomerId) ?? null;

  const openEditor = (row: PriceRow): void => {
    const effectiveFrom: string = viewDate < today ? today : viewDate;
    setFeedback(null);
    priceMutation.reset();
    setDraft({
      employeeId: row.employeeId,
      customerHourlyRate: row.customerBill.rate?.hourly_rate ?? "",
      workerHourlyRate: row.workerPay.rate?.hourly_rate ?? "",
      currency: row.customerBill.rate?.currency ?? row.workerPay.rate?.currency ?? "VND",
      effectiveFrom,
    });
  };

  const submitPrices = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    if (draft === null || selectedCustomerId === "") return;
    setFeedback(null);
    priceMutation.mutate({
      customer_id: selectedCustomerId,
      employee_id: draft.employeeId,
      currency: draft.currency.trim().toLocaleUpperCase("en-US"),
      customer_hourly_rate: draft.customerHourlyRate.trim(),
      worker_hourly_rate: draft.workerHourlyRate.trim(),
      effective_from: draft.effectiveFrom,
    });
  };

  if (!canRead) {
    return <section className="panel p-8 text-center text-sm text-slate-500">Bạn chưa có quyền xem giá và tiền công.</section>;
  }

  const loading: boolean = ratesQuery.isPending || staffQuery.isPending || customersQuery.isPending;
  if (loading) {
    return <section className="panel p-8 text-center text-sm text-slate-500"><LoaderCircle className="mr-2 inline size-4 animate-spin" />Đang tải giá và tiền công...</section>;
  }

  const error: Error | null = ratesQuery.error ?? staffQuery.error ?? customersQuery.error ?? null;
  const editingRow: PriceRow | null = draft === null
    ? null
    : rows.find((row: PriceRow): boolean => row.employeeId === draft.employeeId) ?? null;

  return (
    <div className="space-y-5">
      {feedback ? <div className="rounded-xl bg-emerald-50 px-4 py-3 text-sm font-medium text-emerald-800">{feedback}</div> : null}
      {error ? <div className="rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(error, "Không thể tải giá và tiền công.")}</div> : null}

      <section className="panel p-5 sm:p-6">
        <div className="flex flex-col gap-5 lg:flex-row lg:items-end lg:justify-between">
          <div className="max-w-2xl">
            <div className="flex items-center gap-2">
              <BadgeDollarSign className="size-5 text-blue-600" />
              <h2 className="font-black text-slate-950">Bảng giá theo khách hàng và Staff</h2>
            </div>
            <p className="mt-1 text-sm text-slate-500">
              So sánh trực tiếp giá thu và tiền công theo giờ. Dòng mặc định áp dụng khi Staff chưa có mức riêng.
            </p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2 lg:min-w-[34rem]">
            <label className="text-sm font-semibold text-slate-700">
              Khách hàng
              <select className="mt-1.5 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setSelectedCustomerId(event.target.value)} value={selectedCustomerId}>
                {activeCustomers.length === 0 ? <option value="">Chưa có khách hàng đang hoạt động</option> : null}
                {activeCustomers.map((customer: Customer): React.JSX.Element => <option key={customer.id} value={customer.id}>{customer.name}</option>)}
              </select>
            </label>
            <label className="text-sm font-semibold text-slate-700">
              Xem giá tại ngày
              <div className="relative mt-1.5">
                <CalendarDays className="pointer-events-none absolute left-3 top-3 size-5 text-slate-400" />
                <input className="min-h-11 w-full rounded-xl border-slate-300 pl-10" onChange={(event): void => setViewDate(event.target.value)} required type="date" value={viewDate} />
              </div>
            </label>
          </div>
        </div>

        <div className="mt-5 flex flex-col gap-3 border-t border-slate-100 pt-5 sm:flex-row sm:items-center sm:justify-between">
          <div className="relative w-full sm:max-w-sm">
            <Search className="pointer-events-none absolute left-3 top-3 size-5 text-slate-400" />
            <input className="min-h-11 w-full rounded-xl border-slate-300 pl-10" onChange={(event): void => setSearch(event.target.value)} placeholder="Tìm Staff theo tên hoặc mã..." value={search} />
          </div>
          <p className="text-xs leading-5 text-slate-500">
            Giá quá khứ chỉ để xem. Mỗi thay đổi từ hôm nay trở đi tạo một phiên bản mới và giữ nguyên lịch sử.
          </p>
        </div>
      </section>

      <section className="panel overflow-hidden">
        {selectedCustomer === null ? (
          <div className="p-10 text-center text-sm text-slate-500">Hãy tạo một khách hàng đang hoạt động trước khi thiết lập giá.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="min-w-full divide-y divide-slate-200">
              <thead className="bg-slate-50">
                <tr className="text-left text-xs font-black uppercase tracking-wide text-slate-500">
                  <th className="px-5 py-3">Staff</th>
                  <th className="px-5 py-3">Giá thu khách hàng</th>
                  <th className="px-5 py-3">Tiền công nhân viên</th>
                  <th className="px-5 py-3">Hiệu lực từ</th>
                  <th className="px-5 py-3 text-right">Thao tác</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100 bg-white">
                {visibleRows.map((row: PriceRow): React.JSX.Element => (
                  <tr className={row.employeeId === null ? "bg-blue-50/40" : "hover:bg-slate-50"} key={row.employeeId ?? "all-staff"}>
                    <td className="px-5 py-4">
                      <div className="flex items-center gap-3">
                        <div className={`grid size-10 shrink-0 place-items-center rounded-xl ${row.employeeId === null ? "bg-blue-100 text-blue-700" : "bg-slate-100 text-slate-600"}`}>
                          <UsersRound className="size-5" />
                        </div>
                        <div>
                          <p className="font-bold text-slate-950">{row.displayName}</p>
                          <p className="mt-0.5 text-xs text-slate-500">{row.employeeCode ?? "Mức mặc định"}</p>
                        </div>
                      </div>
                    </td>
                    <td className="px-5 py-4"><RateCell price={row.customerBill} /></td>
                    <td className="px-5 py-4"><RateCell price={row.workerPay} /></td>
                    <td className="max-w-52 px-5 py-4 text-sm font-semibold text-slate-600">{effectiveLabel(row)}</td>
                    <td className="px-5 py-4 text-right">
                      {canManage ? (
                        <button className="action-secondary min-h-9 px-3" onClick={(): void => openEditor(row)} type="button">
                          <Pencil className="size-4" />
                          {row.customerBill.rate === null && row.workerPay.rate === null ? "Thiết lập" : "Thay đổi"}
                        </button>
                      ) : null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {visibleRows.length === 0 ? <p className="p-8 text-center text-sm text-slate-500">Không tìm thấy Staff phù hợp.</p> : null}
          </div>
        )}
        <CursorPagination currentItemCount={visibleRows.length} currentPage={currentPage} hasNextPage={hasNextPage} nextPagePending={staffQuery.isFetchingNextPage} onPageChange={changePage} />
      </section>

      {draft !== null && editingRow !== null && selectedCustomer !== null ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/50 p-4">
          <form className="w-full max-w-xl rounded-2xl bg-white p-6 shadow-2xl" onSubmit={submitPrices}>
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2 className="text-xl font-black text-slate-950">Thiết lập giá và tiền công</h2>
                <p className="mt-1 text-sm text-slate-500">{selectedCustomer.name} · {editingRow.displayName}</p>
              </div>
              <button aria-label="Đóng" className="grid size-9 place-items-center rounded-lg hover:bg-slate-100" onClick={(): void => setDraft(null)} type="button"><X className="size-5" /></button>
            </div>

            <div className="mt-5 rounded-xl bg-blue-50 px-4 py-3 text-sm leading-6 text-blue-800">
              Hệ thống không sửa số tiền của bản ghi cũ. Khi lưu, mức hiện tại được khép lại và mức mới bắt đầu từ ngày bạn chọn.
            </div>

            <div className="mt-5 grid gap-4 sm:grid-cols-2">
              <label className="text-sm font-semibold text-slate-700">
                Giá thu khách hàng / giờ
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" inputMode="decimal" min="0.0001" onChange={(event): void => setDraft({ ...draft, customerHourlyRate: event.target.value })} required step="0.0001" type="number" value={draft.customerHourlyRate} />
              </label>
              <label className="text-sm font-semibold text-slate-700">
                Tiền công nhân viên / giờ
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" inputMode="decimal" min="0.0001" onChange={(event): void => setDraft({ ...draft, workerHourlyRate: event.target.value })} required step="0.0001" type="number" value={draft.workerHourlyRate} />
              </label>
              <label className="text-sm font-semibold text-slate-700">
                Ngày bắt đầu hiệu lực
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" min={today} onChange={(event): void => setDraft({ ...draft, effectiveFrom: event.target.value })} required type="date" value={draft.effectiveFrom} />
              </label>
              <label className="text-sm font-semibold text-slate-700">
                Tiền tệ
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={3} onChange={(event): void => setDraft({ ...draft, currency: event.target.value })} required value={draft.currency} />
              </label>
            </div>

            {priceMutation.error ? <p className="mt-4 rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(priceMutation.error, "Không thể lưu mức giá. Ngày hiệu lực phải là hôm nay hoặc tương lai.")}</p> : null}
            <div className="mt-6 flex justify-end gap-3">
              <button className="action-secondary" onClick={(): void => setDraft(null)} type="button">Hủy</button>
              <button className="action-primary" disabled={priceMutation.isPending} type="submit">
                {priceMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : null}
                Lưu phiên bản mới
              </button>
            </div>
          </form>
        </div>
      ) : null}
    </div>
  );
}
