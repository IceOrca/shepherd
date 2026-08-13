import { LogIn, ShieldCheck, UsersRound } from "lucide-react";
import { Navigate, useLocation } from "react-router-dom";
import { useAuth } from "./AuthProvider";

interface LoginLocationState {
  from?: {
    pathname?: string;
  };
}

export function LoginPage() {
  const auth = useAuth();
  const location = useLocation();

  if (auth.status === "authenticated") {
    return <Navigate to="/dashboard" replace />;
  }

  const state = location.state as LoginLocationState | null;
  const returnTo = state?.from?.pathname || "/dashboard";

  return (
    <main className="min-h-screen bg-slate-950 lg:grid lg:grid-cols-[minmax(0,1.05fr)_minmax(520px,0.95fr)]">
      <section className="relative hidden overflow-hidden p-12 text-white lg:flex lg:flex-col lg:justify-between">
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_20%_10%,rgba(59,130,246,0.30),transparent_36%),radial-gradient(circle_at_85%_80%,rgba(14,165,233,0.18),transparent_32%)]" />
        <div className="relative flex items-center gap-3">
          <div className="grid size-11 place-items-center rounded-xl bg-blue-500 text-xl font-black shadow-lg shadow-blue-500/25">S</div>
          <div>
            <p className="text-lg font-bold">Shepherd</p>
            <p className="text-sm text-slate-400">Điều hành nhân sự theo ca</p>
          </div>
        </div>

        <div className="relative max-w-xl">
          <span className="mb-5 inline-flex rounded-full border border-blue-400/25 bg-blue-400/10 px-3 py-1 text-xs font-semibold text-blue-200">
            Dành cho doanh nghiệp cung ứng nhân sự
          </span>
          <h1 className="text-4xl font-bold leading-tight xl:text-5xl">Mọi ca làm, con người và địa điểm trong một nơi dễ hiểu.</h1>
          <p className="mt-6 max-w-lg text-lg leading-8 text-slate-300">
            Theo dõi nhân viên đang làm việc, xử lý ca phát sinh và đối soát thời gian mà không cần một hệ thống ERP phức tạp.
          </p>
        </div>

        <div className="relative grid grid-cols-2 gap-4">
          <div className="rounded-2xl border border-white/10 bg-white/5 p-5 backdrop-blur">
            <UsersRound className="mb-3 size-6 text-sky-300" />
            <p className="font-semibold">Thân thiện với nhân viên</p>
            <p className="mt-1 text-sm text-slate-400">Bắt đầu và kết thúc ca chỉ với một nút.</p>
          </div>
          <div className="rounded-2xl border border-white/10 bg-white/5 p-5 backdrop-blur">
            <ShieldCheck className="mb-3 size-6 text-emerald-300" />
            <p className="font-semibold">Đăng nhập an toàn</p>
            <p className="mt-1 text-sm text-slate-400">Danh tính và phiên làm việc được Keycloak bảo vệ.</p>
          </div>
        </div>
      </section>

      <section className="flex min-h-screen items-center justify-center bg-slate-50 px-5 py-10 sm:px-10">
        <div className="w-full max-w-md">
          <div className="mb-9 flex items-center gap-3 lg:hidden">
            <div className="grid size-10 place-items-center rounded-xl bg-blue-600 text-lg font-black text-white">S</div>
            <div>
              <p className="font-bold text-slate-950">Shepherd</p>
              <p className="text-xs text-slate-500">Điều hành nhân sự theo ca</p>
            </div>
          </div>

          <div className="rounded-3xl border border-slate-200 bg-white p-6 shadow-xl shadow-slate-200/60 sm:p-9">
            <p className="text-sm font-semibold text-blue-600">Chào mừng trở lại</p>
            <h2 className="mt-2 text-2xl font-bold tracking-tight text-slate-950">Đăng nhập Shepherd</h2>
            <p className="mt-3 text-sm leading-6 text-slate-500">
              Bạn sẽ được chuyển tới dịch vụ đăng nhập bảo mật của công ty. Shepherd không lưu mật khẩu của bạn.
            </p>

            <button
              className="mt-8 flex min-h-12 w-full items-center justify-center gap-2 rounded-xl bg-blue-600 px-4 font-bold text-white shadow-lg shadow-blue-600/20 transition hover:bg-blue-700"
              onClick={() => auth.login(returnTo)}
              type="button"
            >
              <LogIn className="size-5" />
              Tiếp tục đăng nhập
            </button>

            <p className="mt-6 text-center text-xs leading-5 text-slate-400">
              Chỉ tài khoản đã được công ty cấp quyền mới có thể truy cập hệ thống.
            </p>
          </div>
        </div>
      </section>
    </main>
  );
}
