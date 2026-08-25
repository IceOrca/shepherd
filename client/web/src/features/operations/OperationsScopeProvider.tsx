import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { Building2, MapPin, RefreshCw } from "lucide-react";
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

const ALL_BRANCHES: string = "all";
const COORDINATION_ROLES: ReadonlySet<RoleCode> = new Set<RoleCode>([
  "tenant_owner",
  "executive_manager",
  "branch_manager",
  "supervisor",
]);

export interface OperationsScopeCustomer extends UrgentWorkCustomer {
  branch_id: string;
  branch_name: string;
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
  selectBranch(branchId: string | null): void;
  selectCustomer(customerId: string | null): void;
}

interface OperationsScopeProviderProps {
  branches: BranchSummary[];
  children: ReactNode;
}

const OperationsScopeContext: React.Context<OperationsScopeValue | null> =
  createContext<OperationsScopeValue | null>(null);

async function loadCustomersForBranches(
  branches: BranchSummary[],
): Promise<OperationsScopeCustomer[]> {
  const customerGroups: OperationsScopeCustomer[][] = await Promise.all(
    branches.map(async (branch: BranchSummary): Promise<OperationsScopeCustomer[]> => {
      const customers: Customer[] = await listCustomersForBranch(branch.id);
      return customers.map(
        (customer: Customer): OperationsScopeCustomer => ({
          customer_id: customer.id,
          customer_name: customer.name,
          address: customer.address,
          time_zone: customer.time_zone,
          branch_id: branch.id,
          branch_name: branch.name,
        }),
      );
    }),
  );
  return customerGroups
    .flat()
    .sort((left: OperationsScopeCustomer, right: OperationsScopeCustomer): number =>
      left.customer_name.localeCompare(right.customer_name, "vi"),
    );
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
  const [branchSelection, setBranchSelection] = useState<string>(ALL_BRANCHES);
  const [selectedCustomerId, setSelectedCustomerId] = useState<string | null>(null);

  useEffect((): void => {
    if (branchSelection === ALL_BRANCHES) {
      return;
    }
    if (!branches.some((branch: BranchSummary): boolean => branch.id === branchSelection)) {
      console.warn("Operations result branch selection is no longer authorized", {
        branchId: branchSelection,
      });
      setBranchSelection(ALL_BRANCHES);
    }
  }, [branchSelection, branches]);

  const selectedBranches: BranchSummary[] = useMemo<BranchSummary[]>(
    (): BranchSummary[] =>
      branchSelection === ALL_BRANCHES
        ? branches
        : branches.filter((branch: BranchSummary): boolean => branch.id === branchSelection),
    [branchSelection, branches],
  );
  const branchIds: string[] = useMemo<string[]>(
    (): string[] => selectedBranches.map((branch: BranchSummary): string => branch.id),
    [selectedBranches],
  );
  const scopeKey: string = branchIds.join(",") || "none";
  const customersQuery: UseQueryResult<OperationsScopeCustomer[], Error> = useQuery({
    queryKey: ["operations", "result-scope", "customers", scopeKey],
    queryFn: (): Promise<OperationsScopeCustomer[]> => loadCustomersForBranches(selectedBranches),
    enabled: canFilterResults && selectedBranches.length > 0,
    staleTime: 60_000,
  });
  const customers: OperationsScopeCustomer[] = customersQuery.data ?? [];

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
      selectedBranchId: branchSelection === ALL_BRANCHES ? null : branchSelection,
      selectedCustomerId,
      customers,
      scopeKey,
      customerError: customersQuery.error,
      customersPending: customersQuery.isPending,
      selectBranch(branchId: string | null): void {
        const nextSelection: string = branchId ?? ALL_BRANCHES;
        console.info("Operations result branch filter changed", { branchId });
        setBranchSelection(nextSelection);
        setSelectedCustomerId(null);
      },
      selectCustomer(customerId: string | null): void {
        console.info("Operations result customer filter changed", { customerId });
        setSelectedCustomerId(customerId);
      },
    }),
    [
      branchIds,
      branchSelection,
      branches,
      canFilterResults,
      customers,
      customersQuery.error,
      customersQuery.isPending,
      scopeKey,
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
        <h2 className="font-bold text-slate-950">Bộ lọc kết quả</h2>
        <p className="mt-1 text-xs text-slate-500">
          Chỉ thay đổi dữ liệu đang xem; không đổi chi nhánh thao tác ở thanh trên.
        </p>
      </div>
      <label className="min-w-56 flex-1 text-sm font-semibold text-slate-700">
        <span className="flex items-center gap-2">
          <Building2 className="size-4 text-blue-600" />
          Lọc theo chi nhánh
        </span>
        <select
          className="mt-1.5 w-full rounded-xl border border-slate-200 bg-white px-3 py-2.5"
          onChange={(event: React.ChangeEvent<HTMLSelectElement>): void =>
            scope.selectBranch(event.target.value === ALL_BRANCHES ? null : event.target.value)
          }
          value={scope.selectedBranchId ?? ALL_BRANCHES}
        >
          <option value={ALL_BRANCHES}>Tất cả chi nhánh</option>
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
              {scope.selectedBranchId === null ? ` · ${customer.branch_name}` : ""}
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
