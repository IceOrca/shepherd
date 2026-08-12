import { LoaderCircle } from "lucide-react";
import { Navigate, Outlet, Route, Routes, useLocation } from "react-router-dom";
import { LoginPage } from "../features/auth/LoginPage";
import { useAuth } from "../features/auth/AuthProvider";
import { MyAssignmentsPage } from "../features/operations/MyAssignmentsPage";
import { OperationsOverviewPage } from "../features/operations/OperationsOverviewPage";
import { OperationsLayout } from "../layouts/OperationsLayout";

function SessionGate() {
  const auth = useAuth();
  const location = useLocation();

  if (auth.status === "loading") {
    return (
      <main className="grid min-h-screen place-items-center bg-slate-50">
        <div className="text-center">
          <div className="mx-auto grid size-12 place-items-center rounded-2xl bg-blue-600 text-xl font-black text-white shadow-lg shadow-blue-600/20">
            S
          </div>
          <LoaderCircle className="mx-auto mt-5 size-6 animate-spin text-blue-600" />
          <p className="mt-3 text-sm font-medium text-slate-500">Đang khôi phục phiên làm việc...</p>
        </div>
      </main>
    );
  }

  if (auth.status === "anonymous") {
    return <Navigate to="/dang-nhap" replace state={{ from: location }} />;
  }

  return <Outlet />;
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
      <Route path="/dang-nhap" element={<LoginPage />} />
      <Route element={<SessionGate />}>
        <Route element={<OperationsLayout />}>
          <Route index element={<Navigate to="/tong-quan" replace />} />
          <Route path="/tong-quan" element={<OperationsOverviewPage />} />
          <Route path="/van-hanh/ca-lam-cua-toi" element={<MyAssignmentsPage />} />
          <Route path="/admin/*" element={<Navigate to="/tong-quan" replace />} />
          <Route path="*" element={<NotFoundPage />} />
        </Route>
      </Route>
    </Routes>
  );
}
