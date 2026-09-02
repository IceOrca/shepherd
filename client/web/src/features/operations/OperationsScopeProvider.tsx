import { useInfiniteQuery } from "@tanstack/react-query";
import { MapPin, RefreshCw } from "lucide-react";
import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type {
  BranchSummary,
  Customer,
  PermissionCode,
  RoleCode,
  UrgentWorkCustomer,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { useAuth } from "../auth/AuthProvider";
import { listCustomersForBranch } from "./api";
import {
  createReconciliationScopeCursor,
  loadReconcileScopePage,
  type ReconcileScopePage,
  type ReconciliationScopeCursor,
} from "./reconciliationCursor";

const COORDINATION_ROLES: ReadonlySet<RoleCode> = new Set<RoleCode>([
  "tenant_owner",
  "executive_manager",
  "branch_manager",
  "supervisor",
]);

export interface OperationsScopeCustomer extends UrgentWorkCustomer {
  branch_id: string;
  branch_name: string;
  sort_name: string;
  sort_code: string;
}

interface OperationsScopeValue {
  canFilterResults: boolean;
  branches: BranchSummary[];
  branchIds: string[];
  selectedBranchId: string | null;
  selectedCustomerId: string | null;
  customers: OperationsScopeCustomer[];
  scopeKey: string;
  customerError: unknown;
  customersPending: boolean;
  customersHasMore: boolean;
  customersFetchingMore: boolean;
  loadMoreCustomers(): void;
  selectBranch(branchId: string | null): void;
  selectCustomer(customerId: string | null): void;
}

interface OperationsScopeProviderProps {
  branches: BranchSummary[];
  children: ReactNode;
}

const OperationsScopeContext: React.Context<OperationsScopeValue | null> =
  createContext<OperationsScopeValue | null>(null);

function compareScopeCustomers(left: OperationsScopeCustomer, right: OperationsScopeCustomer): number {
  if (left.sort_name !== right.sort_name) return left.sort_name < right.sort_name ? -1 : 1;
  if (left.sort_code !== right.sort_code) return left.sort_code < right.sort_code ? -1 : 1;
  return left.customer_id.localeCompare(right.customer_id);
}

export function OperationsScopeProvider({
  branches,
  children,
}: OperationsScopeProviderProps): React.JSX.Element {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const role: RoleCode | undefined = auth.profile?.primary_role;
  const permissions: PermissionCode[] = auth.profile?.permissions ?? [];
  const canReadResults: boolean =
    permissions.includes("business.reconciliation.read") ||
    permissions.includes("business.shifts.read");
  const canFilterResults: boolean =
    role !== undefined && COORDINATION_ROLES.has(role) && canReadResults;
  const [selectedResultBranchId, setSelectedResultBranchId] = useState<string | null>(null);
  const [selectedCustomerId, setSelectedCustomerId] = useState<string | null>(null);
  const activeBranchId: string | null = auth.profile?.active_branch_id ?? null;

  const selectedBranches: BranchSummary[] = useMemo<BranchSummary[]>(
    (): BranchSummary[] =>
      canFilterResults
        ? selectedResultBranchId === null
          ? branches
          : branches.filter((branch: BranchSummary): boolean => branch.id === selectedResultBranchId)
        : activeBranchId === null
          ? []
          : branches.filter((branch: BranchSummary): boolean => branch.id === activeBranchId),
    [activeBranchId, branches, canFilterResults, selectedResultBranchId],
  );
  const branchIds: string[] = useMemo<string[]>(
    (): string[] => selectedBranches.map((branch: BranchSummary): string => branch.id),
    [selectedBranches],
  );
  const scopeKey: string = branchIds.join(",") || "none";
  const customersQuery = useInfiniteQuery({
    queryKey: ["operations", "result-scope", "customers", scopeKey],
    initialPageParam: createReconciliationScopeCursor<OperationsScopeCustomer>(branchIds),
    queryFn: ({ pageParam }: { pageParam: ReconciliationScopeCursor<OperationsScopeCustomer> }) =>
      loadReconcileScopePage({
        cursor: pageParam,
        fetchBranchPage: async (branchId: string, cursor: string | null) => {
          const page = await listCustomersForBranch(branchId, cursor);
          const branch: BranchSummary | undefined = selectedBranches.find((item) => item.id === branchId);
          return {
            ...page,
            items: page.items.map((customer: Customer): OperationsScopeCustomer => ({
              customer_id: customer.id,
              customer_name: customer.name,
              address: customer.address,
              time_zone: customer.time_zone,
              branch_id: branchId,
              branch_name: branch?.name ?? branchId,
              sort_name: customer.name.toLowerCase(),
              sort_code: customer.code,
            })),
          };
        },
        compare: compareScopeCustomers,
        itemKey: (customer: OperationsScopeCustomer): string => customer.customer_id,
      }),
    getNextPageParam: (page: ReconcileScopePage<OperationsScopeCustomer>) => page.nextCursor ?? undefined,
    enabled: canFilterResults && selectedBranches.length > 0,
    staleTime: 60_000,
  });
  const customers: OperationsScopeCustomer[] = useMemo(
    () => customersQuery.data?.pages.flatMap((page: ReconcileScopePage<OperationsScopeCustomer>) => page.items) ?? [],
    [customersQuery.data?.pages],
  );

  useEffect((): void => {
    if (
      selectedResultBranchId !== null &&
      !branches.some((branch: BranchSummary): boolean => branch.id === selectedResultBranchId)
    ) {
      setSelectedResultBranchId(null);
    }
  }, [branches, selectedResultBranchId]);

  useEffect((): void => {
    if (
      selectedCustomerId !== null &&
      !customers.some(
        (customer: OperationsScopeCustomer): boolean =>
          customer.customer_id === selectedCustomerId,
      )
    ) {
      console.info("Operations result customer filter reset after branch scope changed", {
        customerId: selectedCustomerId,
        scopeKey,
      });
      setSelectedCustomerId(null);
    }
  }, [customers, scopeKey, selectedCustomerId]);

  const value: OperationsScopeValue = useMemo<OperationsScopeValue>(
    (): OperationsScopeValue => ({
      canFilterResults,
      branches,
      branchIds,
      selectedBranchId: selectedResultBranchId,
      selectedCustomerId,
      customers,
      scopeKey,
      customerError: customersQuery.error,
      customersPending: customersQuery.isPending,
      customersHasMore: customersQuery.hasNextPage,
      customersFetchingMore: customersQuery.isFetchingNextPage,
      loadMoreCustomers(): void {
        void customersQuery.fetchNextPage();
      },
      selectBranch(branchId: string | null): void {
        setSelectedResultBranchId(branchId);
        setSelectedCustomerId(null);
      },
      selectCustomer(customerId: string | null): void {
        console.info("Operations result customer filter changed", { customerId });
        setSelectedCustomerId(customerId);
      },
    }),
    [
      branchIds,
      branches,
      canFilterResults,
      customers,
      customersQuery.error,
      customersQuery.isPending,
      customersQuery.hasNextPage,
      customersQuery.isFetchingNextPage,
      customersQuery.fetchNextPage,
      scopeKey,
      selectedResultBranchId,
      selectedCustomerId,
    ],
  );

  return (
    <OperationsScopeContext.Provider value={value}>
      {children}
    </OperationsScopeContext.Provider>
  );
}

export function OperationsScopeToolbar(): React.JSX.Element | null {
  const scope: OperationsScopeValue = useOperationsScope();
  if (!scope.canFilterResults) {
    return null;
  }

  return (
    <section className="mb-6 flex flex-wrap items-end gap-3 rounded-2xl border border-slate-200 bg-white p-4 shadow-sm">
      <div className="w-full">
        <h2 className="font-bold text-slate-950">Phạm vi kết quả</h2>
        <p className="mt-1 text-xs text-slate-500">
          Đây là phạm vi xem kết quả. Chi nhánh thao tác vẫn được chọn riêng ở thanh điều hướng phía trên.
        </p>
      </div>
      <label className="min-w-56 flex-1 text-sm font-semibold text-slate-700">
        Chi nhánh kết quả
        <select
          className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
          onChange={(event: React.ChangeEvent<HTMLSelectElement>): void =>
            scope.selectBranch(event.target.value || null)
          }
          value={scope.selectedBranchId ?? ""}
        >
          <option value="">Tất cả chi nhánh</option>
          {scope.branches.map((branch: BranchSummary): React.JSX.Element => (
            <option key={branch.id} value={branch.id}>
              {branch.name}
            </option>
          ))}
        </select>
      </label>
      <label className="min-w-56 flex-1 text-sm font-semibold text-slate-700">
        <span className="flex items-center gap-2">
          <MapPin className="size-4 text-emerald-600" />
          Lọc theo khách hàng
        </span>
        <select
          className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
          disabled={scope.customersPending || scope.branchIds.length === 0}
          onChange={(event: React.ChangeEvent<HTMLSelectElement>): void =>
            scope.selectCustomer(event.target.value || null)
          }
          value={scope.selectedCustomerId ?? ""}
        >
          <option value="">Tất cả khách hàng</option>
          {scope.customers.map((customer: OperationsScopeCustomer): React.JSX.Element => (
            <option
              key={`${customer.branch_id}:${customer.customer_id}`}
              value={customer.customer_id}
            >
              {customer.customer_name}
            </option>
          ))}
        </select>
      </label>

      {scope.customersPending ? (
        <p className="flex items-center gap-2 pb-2 text-xs font-medium text-slate-500">
          <RefreshCw className="size-3.5 animate-spin" />
          Đang tải khách hàng...
        </p>
      ) : null}
      {scope.customersHasMore ? (
        <button className="action-secondary min-h-10 px-3" disabled={scope.customersFetchingMore} onClick={scope.loadMoreCustomers} type="button">
          {scope.customersFetchingMore ? "Đang tải..." : "Tải thêm khách hàng"}
        </button>
      ) : null}
      {scope.customerError ? (
        <p className="w-full text-xs font-medium text-red-600">
          {friendlyApiError(scope.customerError, "Không thể tải khách hàng cho phạm vi đã chọn.")}
        </p>
      ) : null}
    </section>
  );
}

export function useOperationsScope(): OperationsScopeValue {
  const context: OperationsScopeValue | null = useContext(OperationsScopeContext);
  if (context === null) {
    throw new Error("useOperationsScope must be used inside OperationsScopeProvider");
  }
  return context;
}
