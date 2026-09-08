import { useState, type FormEvent, type ChangeEvent, type ReactElement } from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { Link, Navigate } from "react-router-dom";
import { Building2, ShieldCheck, SlidersHorizontal } from "lucide-react";
import type { TenantBootstrapRequest, TenantBootstrapResult, ServerLogFilter, ServerLogLevel, ServerLogLevelRequest } from "../../api/generated/contracts";
import { apiRequest, friendlyApiError } from "../../shared/api/client";
import { useAuth } from "../auth/AuthProvider";
import { ProfileMenu } from "../auth/ProfileMenu";

export function SystemAdminPage(): ReactElement {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const [slug, setSlug] = useState<string>("");
  const [name, setName] = useState<string>("");
  const [username, setUsername] = useState<string>("");
  const [email, setEmail] = useState<string>("");
  const [password, setPassword] = useState<string>("");
  const [attempt, setAttempt] = useState<TenantBootstrapRequest | null>(null);
  const [result, setResult] = useState<TenantBootstrapResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<boolean>(false);
  const [level, setLevel] = useState<ServerLogLevel>("info");
  const [logBusy, setLogBusy] = useState<boolean>(false);
  const [logMessage, setLogMessage] = useState<string | null>(null);
  const logQuery: UseQueryResult<ServerLogFilter, Error> = useQuery<ServerLogFilter>({ queryKey: ["platform", "logging"], queryFn: (): Promise<ServerLogFilter> => apiRequest("/api/platform/log-level"), enabled: auth.administrator !== null });
  if (!auth.administrator) return <Navigate to="/dashboard" replace />;
  async function create(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault(); setError(null); setResult(null); setBusy(true);
    const request: TenantBootstrapRequest = attempt ?? { tenant_id: crypto.randomUUID(), idempotency_key: crypto.randomUUID(), tenant_slug: slug.trim(), tenant_display_name: name.trim(), owners: [{ username: username.trim(), email: email.trim(), password }] };
    setAttempt(request);
    try {
      const response: TenantBootstrapResult = await apiRequest("/api/platform/tenants", { method: "POST", body: JSON.stringify(request) });
      setResult(response); setAttempt(null); setPassword(""); setSlug(""); setName(""); setUsername(""); setEmail("");
    } catch (failure: unknown) { setError(friendlyApiError(failure, "Không thể tạo doanh nghiệp.")); }
    finally { setBusy(false); }
  }
  async function updateLog(): Promise<void> {
    setLogBusy(true); setLogMessage(null);
    try {
      const request: ServerLogLevelRequest = { level };
      await apiRequest<ServerLogFilter>("/api/platform/log-level", { method: "PUT", body: JSON.stringify(request) });
      await logQuery.refetch(); setLogMessage("Đã áp dụng ngay cho máy chủ đang chạy.");
    } catch (failure: unknown) { setLogMessage(friendlyApiError(failure, "Không thể cập nhật mức log.")); }
    finally { setLogBusy(false); }
  }
  return <main className="min-h-screen bg-slate-50">
    <header className="flex items-center justify-between border-b bg-white px-5 py-4 lg:px-10"><div className="flex items-center gap-3"><ShieldCheck className="size-8 text-blue-600" /><div><h1 className="text-xl font-bold text-slate-900">Quản trị hệ thống</h1><p className="text-sm text-slate-500">Shepherd · Quản lý nền tảng</p></div></div><div className="flex items-center gap-4">{auth.profile ? <Link className="text-sm text-blue-600" to="/dashboard">Về doanh nghiệp</Link> : null}<ProfileMenu /></div></header>
    <div className="mx-auto grid max-w-6xl gap-6 p-5 lg:grid-cols-[1.4fr_1fr] lg:p-10">
      <section className="panel p-6 lg:p-8"><Building2 className="size-7 text-blue-600" /><h2 className="mt-3 text-xl font-bold">Tạo doanh nghiệp</h2><p className="mt-2 text-sm leading-6 text-slate-500">Khởi tạo doanh nghiệp và chủ sở hữu đầu tiên. Chủ sở hữu đăng nhập để tạo chi nhánh và cấp tài khoản.</p>
        <form className="mt-7 space-y-5" onSubmit={(event: FormEvent<HTMLFormElement>): void => { void create(event); }}>
          <fieldset disabled={busy || attempt !== null} className="space-y-5 disabled:opacity-60">
            <label className="block space-y-2 text-sm font-semibold"><span>Tên doanh nghiệp</span><input required maxLength={200} value={name} onChange={(event: ChangeEvent<HTMLInputElement>): void => setName(event.target.value)} className="field-control" /></label>
            <label className="block space-y-2 text-sm font-semibold"><span>Mã doanh nghiệp</span><input required minLength={2} maxLength={63} pattern="[a-z0-9]+(-[a-z0-9]+)*" placeholder="cong-ty-a" value={slug} onChange={(event: ChangeEvent<HTMLInputElement>): void => setSlug(event.target.value)} className="field-control" /></label>
            <div className="border-t pt-5"><h3 className="font-semibold">Chủ doanh nghiệp đầu tiên</h3><p className="mt-1 text-sm font-normal text-slate-500">Nếu email đã có tài khoản, mật khẩu hiện tại của tài khoản đó được giữ nguyên.</p></div>
            <label className="block space-y-2 text-sm font-semibold"><span>Tên tài khoản</span><input required minLength={3} maxLength={128} value={username} onChange={(event: ChangeEvent<HTMLInputElement>): void => setUsername(event.target.value)} className="field-control" /></label>
            <label className="block space-y-2 text-sm font-semibold"><span>Email đăng nhập</span><input required type="email" autoComplete="off" value={email} onChange={(event: ChangeEvent<HTMLInputElement>): void => setEmail(event.target.value)} className="field-control" /></label>
            <label className="block space-y-2 text-sm font-semibold"><span>Mật khẩu ban đầu</span><input required type="password" autoComplete="new-password" minLength={8} maxLength={1024} value={password} onChange={(event: ChangeEvent<HTMLInputElement>): void => setPassword(event.target.value)} className="field-control" /></label>
          </fieldset>
          {error ? <p role="alert" className="rounded-xl bg-red-50 p-4 text-sm text-red-700">{error}</p> : null}
          {attempt && !busy ? <p className="text-sm text-slate-500">Yêu cầu được giữ trong trang để thử lại an toàn. Không tải lại trang khi kết quả chưa rõ.</p> : null}
          <button className="action-primary w-full" type="submit" disabled={busy}>{busy ? "Đang tạo doanh nghiệp..." : attempt ? "Thử lại cùng yêu cầu" : "Tạo doanh nghiệp và chủ sở hữu"}</button>
          {attempt && !busy ? <button className="action-secondary w-full" type="button" onClick={(): void => { setAttempt(null); setError(null); }}>Sửa thông tin yêu cầu</button> : null}
          {result ? <p role="status" className="rounded-xl bg-emerald-50 p-4 text-sm text-emerald-800">Đã tạo doanh nghiệp <strong>{result.tenant_slug}</strong> với {result.owner_count} chủ sở hữu. Chủ sở hữu có thể đăng nhập ngay.</p> : null}
        </form>
      </section>
      <section className="panel self-start p-6 lg:p-8"><SlidersHorizontal className="size-7 text-blue-600" /><h2 className="mt-3 text-xl font-bold">Mức ghi log máy chủ</h2><p className="mt-2 text-sm leading-6 text-slate-500">Áp dụng ngay, không cần khởi động lại. Khi máy chủ khởi động lại, mức log trở về cấu hình môi trường.</p>
        <label className="mt-6 block space-y-2 text-sm font-semibold"><span>Mức log</span><select className="field-control" value={level} onChange={(event: ChangeEvent<HTMLSelectElement>): void => setLevel(event.target.value as ServerLogLevel)}>{(["error", "warn", "info", "debug", "trace"] as ServerLogLevel[]).map((value: ServerLogLevel): ReactElement => <option key={value} value={value}>{value.toUpperCase()}</option>)}</select></label>
        <button className="action-primary mt-5 w-full" disabled={logBusy || logQuery.isPending || logQuery.isError} type="button" onClick={(): void => { void updateLog(); }}>{logBusy ? "Đang áp dụng..." : "Áp dụng mức log"}</button>
        {logQuery.isError ? <p role="alert" className="mt-4 text-sm text-red-700">Không thể tải mức log hiện tại.</p> : <p className="mt-4 break-words text-xs leading-5 text-slate-500">Bộ lọc hiện tại: {logQuery.data?.filter ?? "Đang tải..."}</p>}
        {logMessage ? <p role="status" className="mt-4 rounded-xl bg-blue-50 p-3 text-sm text-blue-800">{logMessage}</p> : null}
      </section>
    </div>
  </main>;
}
