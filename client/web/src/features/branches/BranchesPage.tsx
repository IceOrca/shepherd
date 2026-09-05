import { useInfiniteQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Building2, Clock3, LoaderCircle, MapPinned, Plus, RefreshCw, Save } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";

import type { Branch, BranchCreateRequest, BranchPageResponse, BranchUpdateRequest } from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { CursorPagination } from "../../shared/components/CursorPagination";
import { authAdminQueryKeys } from "../admin/api";
import { useAuth } from "../auth/AuthProvider";
import { operationsQueryKeys } from "../operations/api";
import { branchQueryKeys, createBranch, listManagedBranches, updateBranch } from "./api";

interface Feedback {
  kind: "success" | "error";
  message: string;
}

interface BranchEditor {
  id: string;
  name: string;
  time_zone: string;
  status: string;
  expected_version: number;
}

const emptyCreateRequest: BranchCreateRequest = {
  code: "",
  name: "",
  time_zone: "Asia/Ho_Chi_Minh",
};

function editorFor(branch: Branch): BranchEditor {
  return {
    id: branch.id,
    name: branch.name,
    time_zone: branch.time_zone,
    status: branch.status,
    expected_version: branch.version,
  };
}

export function BranchesPage() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const [createRequest, setCreateRequest] = useState<BranchCreateRequest>(emptyCreateRequest);
  const [selectedBranchId, setSelectedBranchId] = useState<string | null>(null);
  const [editor, setEditor] = useState<BranchEditor | null>(null);
  const [feedback, setFeedback] = useState<Feedback | null>(null);
  const [page, setPage] = useState<number>(1);

  const branchesQuery = useInfiniteQuery<BranchPageResponse>({
    queryKey: branchQueryKeys.all,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }): Promise<BranchPageResponse> => listManagedBranches(pageParam as string | null),
    getNextPageParam: (lastPage: BranchPageResponse): string | undefined => lastPage.next_cursor ?? undefined,
  });
  const pages: BranchPageResponse[] = branchesQuery.data?.pages ?? [];
  const branches: Branch[] = pages[page - 1]?.items ?? [];

  const changePage = async (nextPage: number): Promise<void> => {
    if (nextPage < 1) return;
    if (nextPage > pages.length && branchesQuery.hasNextPage) {
      const result = await branchesQuery.fetchNextPage();
      if ((result.data?.pages.length ?? 0) < nextPage) return;
    }
    setPage(nextPage);
  };

  useEffect((): void => {
    const selected: Branch | undefined = branches.find((branch: Branch): boolean => branch.id === selectedBranchId);
    if (!selected && branches.length > 0) {
      setSelectedBranchId(branches[0].id);
      setEditor(editorFor(branches[0]));
      return;
    }
    if (selected) {
      setEditor(editorFor(selected));
    } else if (branches.length === 0) {
      setSelectedBranchId(null);
      setEditor(null);
    }
  }, [branches, selectedBranchId]);

  const refreshBranchContext = async (): Promise<void> => {
    await auth.refreshProfile();
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: branchQueryKeys.all }),
      queryClient.invalidateQueries({ queryKey: operationsQueryKeys.branches }),
      queryClient.invalidateQueries({ queryKey: authAdminQueryKeys.accessControl }),
    ]);
  };

  const createMutation = useMutation({
    mutationFn: createBranch,
    onSuccess: async (created: Branch): Promise<void> => {
      setFeedback({ kind: "success", message: `Đã tạo chi nhánh ${created.name}.` });
      setCreateRequest(emptyCreateRequest);
      setSelectedBranchId(created.id);
      setEditor(editorFor(created));
      await refreshBranchContext();
    },
    onError: (error: Error): void => {
      setFeedback({
        kind: "error",
        message: friendlyApiError(error, "Không thể tạo chi nhánh. Hãy kiểm tra mã và múi giờ rồi thử lại."),
      });
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, request }: { id: string; request: BranchUpdateRequest }): Promise<Branch> =>
      updateBranch(id, request),
    onSuccess: async (updated: Branch): Promise<void> => {
      setFeedback({ kind: "success", message: `Đã cập nhật chi nhánh ${updated.name}.` });
      setEditor(editorFor(updated));
      await refreshBranchContext();
    },
    onError: (error: Error): void => {
      setFeedback({
        kind: "error",
        message: friendlyApiError(error, "Không thể cập nhật chi nhánh. Dữ liệu có thể đã thay đổi; hãy tải lại."),
      });
    },
  });

  const submitCreate = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    setFeedback(null);
    createMutation.mutate(createRequest);
  };

  const submitUpdate = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    if (!editor) return;
    setFeedback(null);
    updateMutation.mutate({
      id: editor.id,
      request: {
        name: editor.name,
        time_zone: editor.time_zone,
        status: editor.status,
        expected_version: editor.expected_version,
      },
    });
  };

  if (branchesQuery.isLoading) {
    return <div className="panel p-8 text-center text-sm font-semibold text-slate-500"><LoaderCircle className="mx-auto mb-3 size-6 animate-spin text-blue-600" />Đang tải danh sách chi nhánh...</div>;
  }

  if (branchesQuery.isError) {
    return (
      <div className="panel p-8 text-center">
        <Building2 className="mx-auto size-9 text-slate-300" />
        <h2 className="mt-4 text-lg font-bold text-slate-900">Chưa thể tải chi nhánh</h2>
        <p className="mt-2 text-sm text-slate-500">{friendlyApiError(branchesQuery.error, "Máy chủ chưa thể trả dữ liệu chi nhánh.")}</p>
        <button className="action-secondary mt-5" onClick={() => void branchesQuery.refetch()} type="button"><RefreshCw className="size-4" />Thử lại</button>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <section className="surface-card p-5 sm:p-7">
        <div className="flex items-start gap-4">
          <div className="grid size-12 shrink-0 place-items-center rounded-2xl bg-blue-50 text-blue-700"><Building2 className="size-6" /></div>
          <div>
            <p className="text-xs font-bold uppercase tracking-[0.18em] text-blue-600">Cấu trúc doanh nghiệp</p>
            <h1 className="mt-2 text-2xl font-bold text-slate-950">Quản lý chi nhánh</h1>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-500">Tạo đơn vị vận hành nội bộ của doanh nghiệp. Mỗi khách hàng, hồ sơ nhân sự và giao dịch nghiệp vụ sau đó thuộc đúng một chi nhánh.</p>
          </div>
        </div>
      </section>

      {feedback ? <div className={`rounded-2xl border px-4 py-3 text-sm font-semibold ${feedback.kind === "success" ? "border-emerald-200 bg-emerald-50 text-emerald-800" : "border-red-200 bg-red-50 text-red-800"}`}>{feedback.message}</div> : null}

      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_minmax(360px,440px)]">
        <section className="surface-card overflow-hidden">
          <div className="border-b border-slate-100 px-5 py-4 sm:px-6">
            <h2 className="font-bold text-slate-950">Danh sách chi nhánh</h2>
            <p className="mt-1 text-sm text-slate-500">Chọn một chi nhánh để đổi tên, múi giờ hoặc trạng thái.</p>
          </div>
          <div className="divide-y divide-slate-100">
            {branches.map((branch: Branch) => (
              <button
                className={`flex w-full items-center justify-between gap-4 px-5 py-5 text-left transition sm:px-6 ${selectedBranchId === branch.id ? "bg-blue-50" : "hover:bg-slate-50"}`}
                key={branch.id}
                onClick={(): void => {
                  setSelectedBranchId(branch.id);
                  setEditor(editorFor(branch));
                }}
                type="button"
              >
                <span className="min-w-0">
                  <span className="block truncate font-bold text-slate-900">{branch.name}</span>
                  <span className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-slate-500"><span>{branch.code}</span><span className="inline-flex items-center gap-1"><Clock3 className="size-3.5" />{branch.time_zone}</span></span>
                </span>
                <span className={`shrink-0 rounded-full px-2.5 py-1 text-xs font-bold ${branch.status === "active" ? "bg-emerald-50 text-emerald-700" : "bg-slate-100 text-slate-500"}`}>{branch.status === "active" ? "Hoạt động" : "Đã tắt"}</span>
              </button>
            ))}
            {branches.length === 0 ? <div className="px-6 py-12 text-center text-sm text-slate-500"><MapPinned className="mx-auto mb-3 size-8 text-slate-300" />Chưa có chi nhánh nào.</div> : null}
          </div>
          <CursorPagination currentItemCount={branches.length} currentPage={page} hasNextPage={page < pages.length || branchesQuery.hasNextPage} nextPagePending={branchesQuery.isFetchingNextPage} onPageChange={(nextPage: number): void => { void changePage(nextPage); }} />
        </section>

        <div className="space-y-6">
          <form className="surface-card p-5 sm:p-6" onSubmit={submitCreate}>
            <h2 className="text-lg font-bold text-slate-950">Tạo chi nhánh mới</h2>
            <p className="mt-2 text-sm leading-6 text-slate-500">Chi nhánh mới được kích hoạt ngay và xuất hiện trong bộ chọn chi nhánh của tài khoản có phạm vi toàn doanh nghiệp.</p>
            <div className="mt-6 space-y-5">
              <label className="block">
                <span className="text-sm font-bold text-slate-800">Mã chi nhánh</span>
                <span className="mt-1 block text-xs leading-5 text-slate-500">2–63 ký tự: chữ Latin, số, dấu gạch ngang hoặc gạch dưới. Mã không đổi sau khi tạo.</span>
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={63} pattern="[A-Za-z0-9][A-Za-z0-9_-]*[A-Za-z0-9]" placeholder="hcm-01" required value={createRequest.code} onChange={(event): void => setCreateRequest({ ...createRequest, code: event.target.value })} />
              </label>
              <label className="block">
                <span className="text-sm font-bold text-slate-800">Tên chi nhánh</span>
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={200} placeholder="Chi nhánh Hồ Chí Minh" required value={createRequest.name} onChange={(event): void => setCreateRequest({ ...createRequest, name: event.target.value })} />
              </label>
              <label className="block">
                <span className="text-sm font-bold text-slate-800">Múi giờ IANA</span>
                <span className="mt-1 block text-xs leading-5 text-slate-500">Dùng để xác định ngày làm việc, kỳ giá và kỳ lương tại địa phương.</span>
                <input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={64} placeholder="Asia/Ho_Chi_Minh" required value={createRequest.time_zone} onChange={(event): void => setCreateRequest({ ...createRequest, time_zone: event.target.value })} />
              </label>
            </div>
            <button className="action-primary mt-6 w-full" disabled={createMutation.isPending} type="submit">{createMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : <Plus className="size-4" />}Tạo chi nhánh</button>
          </form>

          {editor ? (
            <form className="surface-card p-5 sm:p-6" onSubmit={submitUpdate}>
              <h2 className="text-lg font-bold text-slate-950">Cập nhật chi nhánh</h2>
              <p className="mt-2 text-sm text-slate-500">Mã chi nhánh được giữ cố định để bảo toàn tham chiếu nghiệp vụ.</p>
              <div className="mt-6 space-y-5">
                <label className="block"><span className="text-sm font-bold text-slate-800">Tên chi nhánh</span><input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={200} required value={editor.name} onChange={(event): void => setEditor({ ...editor, name: event.target.value })} /></label>
                <label className="block"><span className="text-sm font-bold text-slate-800">Múi giờ IANA</span><input className="mt-2 min-h-11 w-full rounded-xl border-slate-300" maxLength={64} required value={editor.time_zone} onChange={(event): void => setEditor({ ...editor, time_zone: event.target.value })} /></label>
                <label className="block"><span className="text-sm font-bold text-slate-800">Trạng thái</span><select className="mt-2 min-h-11 w-full rounded-xl border-slate-300" value={editor.status} onChange={(event): void => setEditor({ ...editor, status: event.target.value })}><option value="active">Hoạt động</option><option value="disabled">Vô hiệu hóa</option></select></label>
              </div>
              <button className="action-primary mt-6 w-full" disabled={updateMutation.isPending} type="submit">{updateMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : <Save className="size-4" />}Lưu thay đổi</button>
            </form>
          ) : null}
        </div>
      </div>
    </div>
  );
}
