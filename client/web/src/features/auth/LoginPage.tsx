import { Eye, EyeOff, LoaderCircle, ShieldCheck, UsersRound } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Navigate, useLocation, useNavigate } from "react-router-dom";
import type { AuthRequest } from "../../api/generated/contracts";
import { ApiError, friendlyApiError } from "../../shared/api/client";
import { useAuth } from "./AuthProvider";

interface LoginLocationState {
  from?: {
    pathname?: string;
  };
}

function invalidCredentialMessage(error: ApiError): string {
  if (typeof error.payload !== "object" || error.payload === null) {
    return "Thông tin đăng nhập chưa đúng.";
  }

  const remaining = Reflect.get(error.payload, "remaining_attempts");
  if (typeof remaining === "number" && remaining > 0) {
    return `Thông tin đăng nhập chưa đúng. Bạn còn ${remaining} lần thử.`;
  }

  return "Thông tin đăng nhập chưa đúng hoặc tài khoản đang tạm khóa.";
}

export function LoginPage() {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const [form, setForm] = useState<AuthRequest>({
    tenant: "",
    username: "",
    passphrase: "",
  });
  const [showPassphrase, setShowPassphrase] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  if (auth.status === "authenticated") {
    return <Navigate to="/tong-quan" replace />;
  }

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSubmitting(true);
    setErrorMessage(null);

    try {
      await auth.login(form);
      const state = location.state as LoginLocationState | null;
      navigate(state?.from?.pathname || "/tong-quan", { replace: true });
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        setErrorMessage(invalidCredentialMessage(error));
      } else {
        setErrorMessage(friendlyApiError(error, "Không thể đăng nhập lúc này. Vui lòng thử lại."));
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <main className="min-h-screen bg-slate-950 lg:grid lg:grid-cols-[minmax(0,1.05fr)_minmax(520px,0.95fr)]">
      <section className="relative hidden overflow-hidden p-12 text-white lg:flex lg:flex-col lg:justify-between">
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_20%_10%,rgba(59,130,246,0.30),transparent_36%),radial-gradient(circle_at_85%_80%,rgba(14,165,233,0.18),transparent_32%)]" />
        <div className="relative flex items-center gap-3">
          <div className="grid size-11 place-items-center rounded-xl bg-blue-500 text-xl font-black shadow-lg shadow-blue-500/25">
            S
          </div>
          <div>
            <p className="text-lg font-bold">Shepherd</p>
            <p className="text-sm text-slate-400">Điều hành nhân sự theo ca</p>
          </div>
        </div>

        <div className="relative max-w-xl">
          <span className="mb-5 inline-flex rounded-full border border-blue-400/25 bg-blue-400/10 px-3 py-1 text-xs font-semibold text-blue-200">
            Dành cho doanh nghiệp cung ứng nhân sự
          </span>
          <h1 className="text-4xl font-bold leading-tight xl:text-5xl">
            Mọi ca làm, con người và địa điểm trong một nơi dễ hiểu.
          </h1>
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
            <p className="font-semibold">Dữ liệu đáng tin cậy</p>
            <p className="mt-1 text-sm text-slate-400">Thời gian được ghi nhận an toàn tại máy chủ.</p>
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
            <div>
              <p className="text-sm font-semibold text-blue-600">Chào mừng trở lại</p>
              <h2 className="mt-2 text-2xl font-bold tracking-tight text-slate-950">Đăng nhập hệ thống</h2>
              <p className="mt-2 text-sm leading-6 text-slate-500">
                Dùng mã doanh nghiệp và tài khoản đã được cấp.
              </p>
            </div>

            <form className="mt-8 space-y-5" onSubmit={submit}>
              <label className="block">
                <span className="mb-2 block text-sm font-semibold text-slate-700">Mã doanh nghiệp</span>
                <input
                  autoCapitalize="none"
                  autoComplete="organization"
                  className="field-control"
                  name="tenant"
                  onChange={(event) => setForm((current) => ({ ...current, tenant: event.target.value }))}
                  placeholder="Ví dụ: acme"
                  required
                  spellCheck={false}
                  value={form.tenant}
                />
              </label>

              <label className="block">
                <span className="mb-2 block text-sm font-semibold text-slate-700">Tên đăng nhập</span>
                <input
                  autoCapitalize="none"
                  autoComplete="username"
                  className="field-control"
                  name="username"
                  onChange={(event) => setForm((current) => ({ ...current, username: event.target.value }))}
                  placeholder="Nhập tên đăng nhập"
                  required
                  spellCheck={false}
                  value={form.username}
                />
              </label>

              <label className="block">
                <span className="mb-2 block text-sm font-semibold text-slate-700">Mật khẩu</span>
                <div className="relative">
                  <input
                    autoComplete="current-password"
                    className="field-control pr-12"
                    name="passphrase"
                    onChange={(event) => setForm((current) => ({ ...current, passphrase: event.target.value }))}
                    placeholder="Nhập mật khẩu"
                    required
                    type={showPassphrase ? "text" : "password"}
                    value={form.passphrase}
                  />
                  <button
                    aria-label={showPassphrase ? "Ẩn mật khẩu" : "Hiện mật khẩu"}
                    className="absolute inset-y-0 right-0 grid w-12 place-items-center text-slate-400 hover:text-slate-700"
                    onClick={() => setShowPassphrase((visible) => !visible)}
                    type="button"
                  >
                    {showPassphrase ? <EyeOff className="size-5" /> : <Eye className="size-5" />}
                  </button>
                </div>
              </label>

              {errorMessage ? (
                <div className="rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm font-medium text-red-700" role="alert">
                  {errorMessage}
                </div>
              ) : null}

              <button
                className="flex min-h-12 w-full items-center justify-center gap-2 rounded-xl bg-blue-600 px-4 font-bold text-white shadow-lg shadow-blue-600/20 transition hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={submitting}
                type="submit"
              >
                {submitting ? <LoaderCircle className="size-5 animate-spin" /> : null}
                {submitting ? "Đang đăng nhập..." : "Đăng nhập"}
              </button>
            </form>

            <p className="mt-6 text-center text-xs leading-5 text-slate-400">
              Phiên làm việc được bảo vệ và tự động gia hạn bằng cookie an toàn.
            </p>
          </div>
        </div>
      </section>
    </main>
  );
}
