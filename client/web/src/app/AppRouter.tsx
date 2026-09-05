import { LoaderCircle } from "lucide-react";
import { lazy, Suspense, type ReactNode } from "react";
import { Navigate, Outlet, Route, Routes, useLocation } from "react-router-dom";
import { AuthUsersPage } from "../features/admin/AuthUsersPage";
import { AccessControlPage } from "../features/admin/AccessControlPage";
import { LoginPage } from "../features/auth/LoginPage";
import { useAuth } from "../features/auth/AuthProvider";
import { BranchesPage } from "../features/branches/BranchesPage";
import { MyAssignmentsPage } from "../features/operations/MyAssignmentsPage";
import { CustomersPage } from "../features/operations/CustomersPage";
import { OperationsOverviewPage } from "../features/operations/OperationsOverviewPage";
import { ReconciliationPage } from "../features/operations/ReconciliationPage";
import { ShiftCoordinationPage } from "../features/operations/ShiftCoordinationPage";
import { StaffingConfigurationPage } from "../features/operations/StaffingConfigurationPage";
import { UrgentReconciliationPage } from "../features/operations/UrgentReconciliationPage";
import { UrgentWorkPage } from "../features/operations/UrgentWorkPage";
import { EmployeesPage } from "../features/people/EmployeesPage";
import { OperationsLayout } from "../layouts/OperationsLayout";
import { PLANNED_STAFFING_ENABLED } from "../shared/lib/features";

const FinancialOperationsPage = lazy(async () => {
  const module = await import("../features/finance/FinancialOperationsPage");
  return { default: module.FinancialOperationsPage };
});
const PayrollAccountingPage = lazy(async () => {
  const module = await import("../features/finance/PayrollAccountingPage");
  return { default: module.PayrollAccountingPage };
});

function DeferredPage({ children }: { children: ReactNode }) {
  return (
    <Suspense fallback={<div className="panel p-8 text-center text-sm font-semibold text-slate-500">Đang tải dữ liệu...</div>}>
      {children}
    </Suspense>
  );
}

function SessionGate() {
  const auth = useAuth();
  const location = useLocation();

  if (auth.status === "loading") {
    return (
      <main className="grid min-h-screen place-items-center bg-slate-50">
        <div className="text-center">
          <div className="mx-auto grid size-12 place-items-center rounded-2xl bg-blue-600 text-xl font-black text-white shadow-lg shadow-blue-600/20">
            NS
          </div>
          <LoaderCircle className="mx-auto mt-5 size-6 animate-spin text-blue-600" />
          <p className="mt-3 text-sm font-medium text-slate-500">Đang khôi phục phiên làm việc...</p>
        </div>
      </main>
    );
  }

  if (auth.status === "anonymous") {
    return <Navigate to="/login" replace state={{ from: location }} />;
  }

  return <Outlet />;
}

function PermissionGate({
  children,
  anyPermission,
}: {
  children: ReactNode;
  anyPermission: string[];
}) {
  const auth = useAuth();
  const permissions: string[] = auth.profile?.permissions ?? [];

  return anyPermission.some((permission: string): boolean => permissions.includes(permission))
    ? children
    : <Navigate to="/dashboard" replace />;
}

function NotFoundPage() {
  return (
    <section className="panel mx-auto max-w-2xl p-8 text-center">
      <p className="text-sm font-bold text-blue-600">404</p>
      <h1 className="mt-2 text-2xl font-bold text-slate-950">Không tìm thấy trang</h1>
      <p className="mt-3 text-sm text-slate-500">Trang này không tồn tại hoặc chưa được đưa vào hệ thống.</p>
    </section>
  );
}

export function AppRouter() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route element={<SessionGate />}>
        <Route element={<OperationsLayout />}>
          <Route index element={<Navigate to="/dashboard" replace />} />
          <Route path="/dashboard" element={<OperationsOverviewPage />} />
          <Route path="/operations/work" element={<UrgentWorkPage />} />
          {PLANNED_STAFFING_ENABLED ? (
            <Route path="/operations/my-shifts" element={<MyAssignmentsPage />} />
          ) : null}
          {PLANNED_STAFFING_ENABLED ? (
            <Route path="/operations/shifts" element={<PermissionGate anyPermission={["business.shifts.read"]}><ShiftCoordinationPage /></PermissionGate>} />
          ) : null}
          <Route path="/operations/customers" element={<PermissionGate anyPermission={["business.customers.read"]}><CustomersPage /></PermissionGate>} />
          <Route path="/operations/employees" element={<PermissionGate anyPermission={["hr.employees.read"]}><EmployeesPage /></PermissionGate>} />
          <Route path="/operations/staffing-configuration" element={<PermissionGate anyPermission={["business.staffing_rates.read"]}><StaffingConfigurationPage /></PermissionGate>} />
          <Route path="/operations/finance" element={<DeferredPage><FinancialOperationsPage /></DeferredPage>} />
          <Route path="/operations/payroll-accounting" element={<PermissionGate anyPermission={["finance.operating_reports.read", "hr.payroll.read", "hr.salary_rates.read"]}><DeferredPage><PayrollAccountingPage /></DeferredPage></PermissionGate>} />
          <Route path="/operations/reconciliation" element={<PermissionGate anyPermission={["business.reconciliation.read"]}><UrgentReconciliationPage /></PermissionGate>} />
          {PLANNED_STAFFING_ENABLED ? (
            <Route path="/operations/reconciliation/planned" element={<PermissionGate anyPermission={["business.reconciliation.read"]}><ReconciliationPage /></PermissionGate>} />
          ) : null}
          <Route path="/admin/auth-users" element={<AuthUsersPage />} />
          <Route path="/admin/branches" element={<PermissionGate anyPermission={["business.branches.manage"]}><BranchesPage /></PermissionGate>} />
          <Route path="/admin/access-control" element={<AccessControlPage />} />
          <Route path="/admin/*" element={<Navigate to="/admin/access-control" replace />} />
          <Route path="*" element={<NotFoundPage />} />
        </Route>
      </Route>
    </Routes>
  );
}
