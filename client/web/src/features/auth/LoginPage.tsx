import {
  Eye,
  EyeOff,
  LoaderCircle,
  LogIn,
  ShieldCheck,
  UsersRound,
} from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { Navigate, useLocation } from "react-router-dom";
import { useAuth } from "./AuthProvider";
import {
  AuthenticationError,
  beginOAuthLogin,
  consumeAuthCallbackError,
  consumeOAuthReturnPath,
  getAuthSettings,
  type OAuthProvider,
} from "./api";

interface LoginLocationState {
  from?: {
    pathname?: string;
  };
}

export function LoginPage() {
  const auth = useAuth();
  const location = useLocation();
  const state = location.state as LoginLocationState | null;
  const [returnTo] = useState(
    () => consumeOAuthReturnPath() ?? state?.from?.pathname ?? "/dashboard",
  );
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [socialProviders, setSocialProviders] = useState<OAuthProvider[]>([]);
  const [errorMessage, setErrorMessage] = useState<string | null>(
    consumeAuthCallbackError,
  );

  useEffect(() => {
    let active = true;
    void getAuthSettings()
      .then((settings) => {
        if (!active) {
          return;
        }
        const enabled: OAuthProvider[] = [];
        if (settings.external?.google) {
          enabled.push("google");
        }
        if (settings.external?.facebook) {
          enabled.push("facebook");
        }
        setSocialProviders(enabled);
      })
      .catch(() => {
        // Password login remains available when provider discovery fails.
      });
    return () => {
      active = false;
    };
  }, []);

  if (auth.status === "authenticated") {
    return <Navigate to={returnTo} replace />;
  }

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSubmitting(true);
    setErrorMessage(null);
    try {
      await auth.login(email.trim(), password);
    } catch (error) {
      setErrorMessage(
        error instanceof AuthenticationError
          ? error.message
          : "Không thể đăng nhập lúc này. Vui lòng thử lại.",
      );
    } finally {
      setSubmitting(false);
    }
  };

  const startSocialLogin = (provider: OAuthProvider) => {
    setErrorMessage(null);
    beginOAuthLogin(provider, returnTo);
  };

  return (
    <main className="min-h-screen bg-slate-950 lg:grid lg:grid-cols-[minmax(0,1.05fr)_minmax(500px,0.95fr)]">
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
            <p className="font-semibold">Supabase Auth</p>
            <p className="mt-1 text-sm text-slate-400">Mật khẩu và phiên làm việc tách khỏi dữ liệu nghiệp vụ.</p>
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
              Dùng tài khoản do doanh nghiệp cấp. Shepherd không lưu mật khẩu của bạn.
            </p>

            {socialProviders.length > 0 ? (
              <>
                <div className="mt-7 grid gap-3 sm:grid-cols-2">
                  {socialProviders.map((provider) => (
                    <button
                      className="flex min-h-11 items-center justify-center gap-2 rounded-xl border border-slate-300 bg-white px-4 text-sm font-semibold text-slate-700 transition hover:border-slate-400 hover:bg-slate-50"
                      key={provider}
                      onClick={() => startSocialLogin(provider)}
                      type="button"
                    >
                      <span
                        className={provider === "google" ? "text-base font-black text-blue-600" : "text-base font-black text-blue-700"}
                        aria-hidden="true"
                      >
                        {provider === "google" ? "G" : "f"}
                      </span>
                      {provider === "google" ? "Google" : "Facebook"}
                    </button>
                  ))}
                </div>
                <div className="my-6 flex items-center gap-3 text-xs font-semibold uppercase tracking-wider text-slate-400">
                  <span className="h-px flex-1 bg-slate-200" />
                  hoặc dùng email
                  <span className="h-px flex-1 bg-slate-200" />
                </div>
              </>
            ) : null}

            <form className={socialProviders.length > 0 ? "space-y-5" : "mt-8 space-y-5"} onSubmit={(event) => void submit(event)}>
              <label className="block">
                <span className="text-sm font-semibold text-slate-700">Email</span>
                <input
                  autoComplete="email"
                  autoFocus
                  className="mt-2 min-h-12 w-full rounded-xl border border-slate-300 px-4 text-slate-950 outline-none transition focus:border-blue-500 focus:ring-4 focus:ring-blue-100"
                  onChange={(event) => setEmail(event.target.value)}
                  placeholder="ten@congty.vn"
                  required
                  type="email"
                  value={email}
                />
              </label>
              <label className="block">
                <span className="text-sm font-semibold text-slate-700">Mật khẩu</span>
                <span className="relative mt-2 block">
                  <input
                    autoComplete="current-password"
                    className="min-h-12 w-full rounded-xl border border-slate-300 px-4 pr-12 text-slate-950 outline-none transition focus:border-blue-500 focus:ring-4 focus:ring-blue-100"
                    minLength={6}
                    onChange={(event) => setPassword(event.target.value)}
                    required
                    type={showPassword ? "text" : "password"}
                    value={password}
                  />
                  <button
                    aria-label={showPassword ? "Ẩn mật khẩu" : "Hiện mật khẩu"}
                    className="absolute inset-y-0 right-0 grid w-12 place-items-center text-slate-400 hover:text-slate-700"
                    onClick={() => setShowPassword((visible) => !visible)}
                    type="button"
                  >
                    {showPassword ? <EyeOff className="size-5" /> : <Eye className="size-5" />}
                  </button>
                </span>
              </label>
              {errorMessage ? (
                <p className="rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700" role="alert">
                  {errorMessage}
                </p>
              ) : null}
              <button
                className="flex min-h-12 w-full items-center justify-center gap-2 rounded-xl bg-blue-600 px-4 font-bold text-white shadow-lg shadow-blue-600/20 transition hover:bg-blue-700 disabled:cursor-wait disabled:opacity-60"
                disabled={submitting || auth.status === "loading"}
                type="submit"
              >
                {submitting || auth.status === "loading" ? (
                  <LoaderCircle className="size-5 animate-spin" />
                ) : (
                  <LogIn className="size-5" />
                )}
                {submitting ? "Đang đăng nhập..." : "Đăng nhập"}
              </button>
            </form>

            <p className="mt-6 text-center text-xs leading-5 text-slate-400">
              Chỉ tài khoản đã được công ty cấp quyền mới có thể truy cập hệ thống.
            </p>
          </div>
        </div>
      </section>
    </main>
  );
}
