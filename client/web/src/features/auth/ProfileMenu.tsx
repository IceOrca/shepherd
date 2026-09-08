import { CircleUserRound, KeyRound, LogOut, ShieldCheck, X } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type ChangeEvent, type ReactElement, type RefObject } from "react";
import { Link } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { useAuth } from "./AuthProvider";
import { changePassword } from "./api";

export function ProfileMenu(): ReactElement {
  const auth: ReturnType<typeof useAuth> = useAuth();
  const cache: ReturnType<typeof useQueryClient> = useQueryClient();
  const dialog: RefObject<HTMLDialogElement | null> = useRef<HTMLDialogElement>(null);
  const menu: RefObject<HTMLDetailsElement | null> = useRef<HTMLDetailsElement>(null);
  const [open, setOpen] = useState<boolean>(false);
  const [current, setCurrent] = useState<string>("");
  const [password, setPassword] = useState<string>("");
  const [confirmation, setConfirmation] = useState<string>("");
  const [busy, setBusy] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<boolean>(false);
  useEffect((): void => {
    if (open) dialog.current?.showModal(); else dialog.current?.close();
  }, [open]);
  function close(): void {
    setOpen(false); setCurrent(""); setPassword(""); setConfirmation(""); setError(null); setSuccess(false);
  }
  async function submit(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (password !== confirmation) { setError("Mật khẩu nhập lại không khớp."); return; }
    if (password === current) { setError("Mật khẩu mới phải khác mật khẩu hiện tại."); return; }
    setBusy(true); setError(null);
    try {
      await changePassword(current, password);
      setCurrent(""); setPassword(""); setConfirmation(""); setSuccess(true);
    } catch (failure: unknown) {
      setError(failure instanceof Error ? failure.message : "Không thể đổi mật khẩu. Vui lòng thử lại.");
    } finally { setBusy(false); }
  }
  return <>
    <details ref={menu} className="relative">
      <summary className="flex cursor-pointer list-none items-center gap-2 rounded-xl p-2 hover:bg-slate-100" aria-label="Hồ sơ tài khoản">
        <span className="hidden text-right text-sm font-semibold text-slate-800 md:block">{auth.profile?.username ?? auth.administrator?.username}</span>
        <CircleUserRound className="size-9 text-slate-500" />
      </summary>
      <div className="absolute right-0 z-50 mt-2 w-64 rounded-2xl border border-slate-200 bg-white p-2 shadow-xl">
        <button type="button" className="flex w-full items-center gap-3 rounded-xl px-4 py-3 text-sm hover:bg-slate-100" onClick={(): void => { if (menu.current) menu.current.open = false; setOpen(true); }}><KeyRound className="size-4" />Đổi mật khẩu</button>
        {auth.administrator ? <Link to="/system-admin" className="flex items-center gap-3 rounded-xl px-4 py-3 text-sm hover:bg-slate-100"><ShieldCheck className="size-4" />Quản trị hệ thống</Link> : null}
        <button type="button" className="flex w-full items-center gap-3 rounded-xl px-4 py-3 text-sm hover:bg-slate-100" onClick={(): void => { void auth.logout().catch((): void => { /* Local session is cleared even if provider logout fails. */ }).finally((): void => cache.clear()); }}><LogOut className="size-4" />Đăng xuất</button>
      </div>
    </details>
    <dialog ref={dialog} onCancel={(event): void => { if (busy) event.preventDefault(); else close(); }} onClose={(): void => close()} className="m-auto w-[calc(100%_-_2rem)] max-w-md rounded-2xl bg-white p-6 shadow-2xl backdrop:bg-slate-950/40">
      <div className="flex items-center justify-between"><h2 className="text-xl font-bold">Đổi mật khẩu</h2><button type="button" aria-label="Đóng" disabled={busy} onClick={close}><X className="size-5" /></button></div>
      <p className="mt-3 text-sm text-slate-500">Nhập mật khẩu hiện tại để xác nhận thay đổi.</p>
      {success ? <div className="mt-5 space-y-4"><p role="status" className="rounded-xl bg-emerald-50 p-4 text-emerald-800">Đã đổi mật khẩu thành công.</p><button type="button" className="action-primary w-full" onClick={close}>Hoàn tất</button></div> :
        <form className="mt-6 space-y-5" onSubmit={(event: FormEvent<HTMLFormElement>): void => { void submit(event); }}>
          <label className="block space-y-2 text-sm font-semibold"><span>Mật khẩu hiện tại</span><input autoFocus required type="password" autoComplete="current-password" value={current} onChange={(event: ChangeEvent<HTMLInputElement>): void => setCurrent(event.target.value)} className="field-control" disabled={busy} maxLength={1024} /></label>
          <label className="block space-y-2 text-sm font-semibold"><span>Mật khẩu mới</span><input required type="password" autoComplete="new-password" minLength={8} maxLength={1024} value={password} onChange={(event: ChangeEvent<HTMLInputElement>): void => setPassword(event.target.value)} className="field-control" disabled={busy} /></label>
          <label className="block space-y-2 text-sm font-semibold"><span>Nhập lại mật khẩu mới</span><input required type="password" autoComplete="new-password" minLength={8} maxLength={1024} value={confirmation} onChange={(event: ChangeEvent<HTMLInputElement>): void => setConfirmation(event.target.value)} className="field-control" disabled={busy} /></label>
          {error ? <p role="alert" className="rounded-xl bg-red-50 p-3 text-sm text-red-700">{error}</p> : null}
          <button type="submit" className="action-primary w-full" disabled={busy}>{busy ? "Đang cập nhật..." : "Đổi mật khẩu"}</button>
        </form>}
    </dialog>
  </>;
}
