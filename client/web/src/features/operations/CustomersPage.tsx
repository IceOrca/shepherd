import {
  useInfiniteQuery,
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { Building2, LoaderCircle, Pencil, Plus, RefreshCw, Search, X } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import type {
  BusinessRecordStatus,
  Customer,
  CustomerPageResponse,
  CustomerUpsertRequest,
  PermissionCode,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { CursorPagination } from "../../shared/components/CursorPagination";
import { useAuth } from "../auth/AuthProvider";
import {
  createCustomer,
  listCustomersPage,
  operationsQueryKeys,
  updateCustomer,
} from "./api";

const emptyDraft: CustomerUpsertRequest = {
  code: "",
  name: "",
  address: null,
  time_zone: "Asia/Bangkok",
  billing_email: null,
  status: "active",
};

interface SaveCustomerVariables {
  customerId: string | null;
  payload: CustomerUpsertRequest;
}

function statusLabel(status: BusinessRecordStatus): string {
  return status === "active" ? "Đang hợp tác" : "Tạm ngừng";
}

function customerDraft(customer: Customer): CustomerUpsertRequest {
  return {
    code: customer.code,
    name: customer.name,
    address: customer.address,
    time_zone: customer.time_zone,
    billing_email: customer.billing_email,
    status: customer.status,
  };
}

export function CustomersPage(): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const queryClient: ReturnType<typeof useQueryClient> = useQueryClient();
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canRead: boolean = permissions.includes("business.customers.read");
  const canManage: boolean = permissions.includes("business.customers.manage");
  const [search, setSearch] = useState<string>("");
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState<CustomerUpsertRequest>(emptyDraft);
  const [formOpen, setFormOpen] = useState<boolean>(false);
  const [feedback, setFeedback] = useState<string | null>(null);

  const customersQuery = useInfiniteQuery({
    queryKey: [...operationsQueryKeys.customers, "page", search.trim()],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }: { pageParam: string | null }): Promise<CustomerPageResponse> =>
      listCustomersPage(pageParam, search),
    getNextPageParam: (lastPage: CustomerPageResponse): string | undefined => lastPage.next_cursor ?? undefined,
    enabled: canRead,
  });
  const loadedPages: CustomerPageResponse[] = customersQuery.data?.pages ?? [];
  const visibleCustomers: Customer[] = loadedPages[currentPage - 1]?.items ?? [];
  const hasNextPage: boolean = currentPage < loadedPages.length || customersQuery.hasNextPage;

  useEffect((): void => setCurrentPage(1), [search]);

  const changePage = (nextPage: number): void => {
    if (nextPage < 1) return;
    if (nextPage <= loadedPages.length) {
      setCurrentPage(nextPage);
      return;
    }
    if (nextPage === loadedPages.length + 1 && customersQuery.hasNextPage) {
      void customersQuery.fetchNextPage().then((result): void => {
        if ((result.data?.pages.length ?? 0) >= nextPage) setCurrentPage(nextPage);
      });
    }
  };

  const saveMutation: UseMutationResult<Customer, Error, SaveCustomerVariables> = useMutation({
    mutationFn: ({ customerId, payload }: SaveCustomerVariables): Promise<Customer> =>
      customerId ? updateCustomer(customerId, payload) : createCustomer(payload),
    onSuccess: (customer: Customer): void => {
      void queryClient.invalidateQueries({ queryKey: operationsQueryKeys.customers });
      setFeedback(editingId ? `Đã cập nhật khách hàng ${customer.name}.` : `Đã tạo khách hàng ${customer.name}.`);
      setFormOpen(false);
      setEditingId(null);
      setDraft(emptyDraft);
    },
  });

  const openCreate: () => void = (): void => {
    setEditingId(null);
    setDraft(emptyDraft);
    setFeedback(null);
    setFormOpen(true);
  };

  const openEdit: (customer: Customer) => void = (customer: Customer): void => {
    setEditingId(customer.id);
    setDraft(customerDraft(customer));
    setFeedback(null);
    setFormOpen(true);
  };

  const submit: (event: FormEvent<HTMLFormElement>) => void = (
    event: FormEvent<HTMLFormElement>,
  ): void => {
    event.preventDefault();
    saveMutation.mutate({
      customerId: editingId,
      payload: {
        ...draft,
        code: draft.code.trim().toLocaleLowerCase("en-US"),
        name: draft.name.trim(),
        address: draft.address?.trim() || null,
        time_zone: draft.time_zone.trim(),
        billing_email: draft.billing_email?.trim().toLocaleLowerCase("en-US") || null,
      },
    });
  };

  if (!canRead) {
    return (
      <section className="panel p-8 text-center">
        <p className="font-bold text-slate-900">Bạn không có quyền xem danh sách khách hàng.</p>
      </section>
    );
  }

  return (
    <section className="space-y-5">
      <div className="panel flex flex-col gap-4 p-5 sm:flex-row sm:items-center sm:justify-between">
        <label className="relative block w-full max-w-xl">
          <Search className="absolute left-3 top-3 size-5 text-slate-400" />
          <input
            className="min-h-11 w-full rounded-xl border-slate-300 pl-10"
            onChange={(event): void => setSearch(event.target.value)}
            placeholder="Tìm theo mã, tên hoặc email thanh toán"
            type="search"
            value={search}
          />
        </label>
        {canManage ? (
          <button className="action-primary shrink-0" onClick={openCreate} type="button">
            <Plus className="size-4" />
            Thêm khách hàng
          </button>
        ) : null}
      </div>

      {feedback ? <div className="rounded-xl bg-emerald-50 px-4 py-3 text-sm font-medium text-emerald-800">{feedback}</div> : null}
      {customersQuery.error ? (
        <div className="rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">
          {friendlyApiError(customersQuery.error, "Không thể tải danh sách khách hàng.")}
        </div>
      ) : null}

      <div className="panel overflow-hidden">
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4">
          <div>
            <h2 className="font-black text-slate-950">Danh sách khách hàng</h2>
            <p className="mt-1 text-sm text-slate-500">{visibleCustomers.length} khách hàng</p>
          </div>
          <button
            aria-label="Tải lại"
            className="grid size-10 place-items-center rounded-xl text-slate-500 hover:bg-slate-100"
            onClick={(): void => {
              void customersQuery.refetch();
            }}
            type="button"
          >
            <RefreshCw className={`size-4 ${customersQuery.isFetching ? "animate-spin" : ""}`} />
          </button>
        </div>
        {customersQuery.isPending ? (
          <div className="grid min-h-48 place-items-center"><LoaderCircle className="size-6 animate-spin text-blue-600" /></div>
        ) : visibleCustomers.length === 0 ? (
          <div className="p-10 text-center text-sm text-slate-500">Chưa có khách hàng phù hợp.</div>
        ) : (
          <div className="divide-y divide-slate-100">
            {visibleCustomers.map((customer: Customer): React.JSX.Element => (
              <article className="flex flex-col gap-3 px-5 py-4 sm:flex-row sm:items-center" key={customer.id}>
                <div className="grid size-11 shrink-0 place-items-center rounded-xl bg-blue-50 text-blue-700">
                  <Building2 className="size-5" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="font-bold text-slate-950">{customer.name}</h3>
                    <span className="rounded-full bg-slate-100 px-2 py-1 text-xs font-bold text-slate-600">{customer.code}</span>
                    <span className={`rounded-full px-2 py-1 text-xs font-bold ${customer.status === "active" ? "bg-emerald-50 text-emerald-700" : "bg-amber-50 text-amber-700"}`}>
                      {statusLabel(customer.status)}
                    </span>
                  </div>
                  <p className="mt-1 text-sm text-slate-500">{customer.address ?? "Chưa có địa chỉ nơi làm việc"}</p>
                  <p className="mt-1 text-xs text-slate-400">{customer.time_zone} · {customer.billing_email ?? "Chưa có email thanh toán"}</p>
                </div>
                {canManage ? (
                  <button className="action-secondary" onClick={(): void => openEdit(customer)} type="button">
                    <Pencil className="size-4" />
                    Chỉnh sửa
                  </button>
                ) : null}
              </article>
            ))}
          </div>
        )}
        <CursorPagination currentItemCount={visibleCustomers.length} currentPage={currentPage} hasNextPage={hasNextPage} nextPagePending={customersQuery.isFetchingNextPage} onPageChange={changePage} />
      </div>

      {formOpen ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/50 p-4">
          <form className="w-full max-w-lg rounded-2xl bg-white p-6 shadow-2xl" onSubmit={submit}>
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2 className="text-xl font-black text-slate-950">{editingId ? "Cập nhật khách hàng" : "Thêm khách hàng"}</h2>
                <p className="mt-1 text-sm text-slate-500">Thông tin này dùng cho điều phối và đối soát.</p>
              </div>
              <button aria-label="Đóng" className="grid size-9 place-items-center rounded-lg hover:bg-slate-100" onClick={(): void => setFormOpen(false)} type="button">
                <X className="size-5" />
              </button>
            </div>
            <div className="mt-6 grid gap-4">
              <label className="text-sm font-semibold text-slate-700">
                Mã khách hàng
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={63} onChange={(event): void => setDraft((current: CustomerUpsertRequest): CustomerUpsertRequest => ({ ...current, code: event.target.value }))} required value={draft.code} />
              </label>
              <label className="text-sm font-semibold text-slate-700">
                Tên khách hàng
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={200} onChange={(event): void => setDraft((current: CustomerUpsertRequest): CustomerUpsertRequest => ({ ...current, name: event.target.value }))} required value={draft.name} />
              </label>
              <label className="text-sm font-semibold text-slate-700">
                Địa chỉ nơi làm việc
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={500} onChange={(event): void => setDraft((current: CustomerUpsertRequest): CustomerUpsertRequest => ({ ...current, address: event.target.value }))} value={draft.address ?? ""} />
              </label>
              <label className="text-sm font-semibold text-slate-700">
                Múi giờ IANA
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={128} onChange={(event): void => setDraft((current: CustomerUpsertRequest): CustomerUpsertRequest => ({ ...current, time_zone: event.target.value }))} required value={draft.time_zone} />
              </label>
              <label className="text-sm font-semibold text-slate-700">
                Email thanh toán
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={320} onChange={(event): void => setDraft((current: CustomerUpsertRequest): CustomerUpsertRequest => ({ ...current, billing_email: event.target.value }))} type="email" value={draft.billing_email ?? ""} />
              </label>
              <label className="text-sm font-semibold text-slate-700">
                Trạng thái
                <select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" onChange={(event): void => setDraft((current: CustomerUpsertRequest): CustomerUpsertRequest => ({ ...current, status: event.target.value as BusinessRecordStatus }))} value={draft.status}>
                  <option value="active">Đang hợp tác</option>
                  <option value="disabled">Tạm ngừng</option>
                </select>
              </label>
            </div>
            {saveMutation.error ? <p className="mt-4 rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">{friendlyApiError(saveMutation.error, "Không thể lưu khách hàng.")}</p> : null}
            <div className="mt-6 flex justify-end gap-3">
              <button className="action-secondary" onClick={(): void => setFormOpen(false)} type="button">Hủy</button>
              <button className="action-primary" disabled={saveMutation.isPending} type="submit">
                {saveMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : null}
                Lưu khách hàng
              </button>
            </div>
          </form>
        </div>
      ) : null}
    </section>
  );
}
