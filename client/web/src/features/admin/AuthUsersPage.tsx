import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckCircle2,
  CircleAlert,
  Eye,
  EyeOff,
  KeyRound,
  LoaderCircle,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  UserRoundCheck,
  UserRoundX,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import type {
  AuthUserSummary,
  AccessControlRole,
  AccessControlSnapshot,
  BranchSummary,
  CreateAuthUserRequest,
  RoleCode,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { roleLabel } from "../../shared/lib/format";
import { useAuth } from "../auth/AuthProvider";
import {
  listBranches,
  operationsQueryKeys,
} from "../operations/api";
import {
  authAdminQueryKeys,
  createAuthUser,
  getAccessControlSnapshot,
  listAuthUsers,
  setAuthUserStatus,
} from "./api";

const emptyCreateRequest: CreateAuthUserRequest = {
  username: "",
  email: "",
  password: "",
  primary_role: "staff",
  branch_ids: [],
};

type CreateRoleOption = {
  code: RoleCode;
  label: string;
  scope: "tenant" | "branch";
  isSystem: boolean;
};

const systemRoleOptions: ReadonlyArray<CreateRoleOption> = [
  { code: "tenant_owner", label: "Chủ doanh nghiệp", scope: "tenant", isSystem: true },
  { code: "executive_manager", label: "Quản lý điều hành", scope: "branch", isSystem: true },
  { code: "branch_manager", label: "Quản lý chi nhánh", scope: "branch", isSystem: true },
  { code: "supervisor", label: "Giám sát", scope: "branch", isSystem: true },
  { code: "staff", label: "Nhân viên", scope: "branch", isSystem: true },
];

function grantableSystemRoleCodes(roles: RoleCode[]): RoleCode[] {
  if (roles.includes("tenant_owner")) {
    return systemRoleOptions.map((role): RoleCode => role.code);
  }
  if (roles.includes("executive_manager")) {
    return ["branch_manager", "supervisor", "staff"];
  }
  if (roles.includes("branch_manager")) {
    return ["supervisor", "staff"];
  }
  if (roles.includes("supervisor")) {
    return ["staff"];
  }
  return [];
}

type CreateAuthUserVariables = {
  request: CreateAuthUserRequest;
  idempotencyKey: string;
};

type CreateAuthUserAttempt = {
  requestSignature: string;
  idempotencyKey: string;
};

function formatOptionalDate(value: string | null): string {
  if (!value) {
    return "Chưa có";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "Không xác định";
  }
  return new Intl.DateTimeFormat("vi-VN", {
    dateStyle: "short",
    timeStyle: "short",
  }).format(date);
}

function userIsDisabled(user: AuthUserSummary): boolean {
  return user.account_status === "disabled";
}

export function AuthUsersPage() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const profile = auth.profile;
  const permissions = profile?.permissions ?? [];
  const canRead = permissions.includes("auth.accounts.read");
  const canCreate = permissions.includes("auth.accounts.create");
  const canDisable = permissions.includes("auth.accounts.disable");
  const canReadRoles = permissions.includes("auth.roles.read");
  const canManageRoles = permissions.includes("auth.roles.manage");
  const [search, setSearch] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [createRequest, setCreateRequest] =
    useState<CreateAuthUserRequest>(emptyCreateRequest);
  const [createAttempt, setCreateAttempt] =
    useState<CreateAuthUserAttempt | null>(null);
  const [feedback, setFeedback] = useState<{
    kind: "success" | "error";
    message: string;
  } | null>(null);

  const usersQuery = useQuery({
    queryKey: authAdminQueryKeys.all,
    queryFn: listAuthUsers,
    enabled: canRead,
  });
  const branchesQuery = useQuery<BranchSummary[]>({
    queryKey: operationsQueryKeys.branches,
    queryFn: listBranches,
    enabled: canCreate,
    staleTime: 60_000,
  });
  const branches: BranchSummary[] = branchesQuery.data ?? [];
  const roleCatalogQuery = useInfiniteQuery({
    queryKey: [...authAdminQueryKeys.accessControl, "account-create-roles"],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }: { pageParam: string | null }): Promise<AccessControlSnapshot> =>
      getAccessControlSnapshot({ roleCursor: pageParam }),
    getNextPageParam: (lastPage: AccessControlSnapshot): string | undefined =>
      lastPage.role_next_cursor ?? undefined,
    enabled: canCreate && canReadRoles,
  });

  useEffect((): void => {
    if (roleCatalogQuery.hasNextPage && !roleCatalogQuery.isFetchingNextPage) {
      void roleCatalogQuery.fetchNextPage();
    }
  }, [
    roleCatalogQuery.hasNextPage,
    roleCatalogQuery.isFetchingNextPage,
    roleCatalogQuery.fetchNextPage,
  ]);

  const roleOptions: CreateRoleOption[] = useMemo((): CreateRoleOption[] => {
    const catalogRoles: AccessControlRole[] =
      roleCatalogQuery.data?.pages.flatMap(
        (page: AccessControlSnapshot): AccessControlRole[] => page.roles,
      ) ?? [];
    if (catalogRoles.length === 0) {
      return [...systemRoleOptions];
    }
    return catalogRoles
      .filter((role: AccessControlRole): boolean => role.is_active)
      .map((role: AccessControlRole): CreateRoleOption => ({
        code: role.code,
        label: role.display_name,
        scope: role.scope,
        isSystem: role.is_system,
      }));
  }, [roleCatalogQuery.data]);
  const grantableSystemRoles: RoleCode[] = grantableSystemRoleCodes(profile?.roles ?? []);
  const grantableRoles: CreateRoleOption[] = roleOptions.filter(
    (role: CreateRoleOption): boolean =>
      role.isSystem ? grantableSystemRoles.includes(role.code) : canManageRoles,
  );
  const selectedRole: CreateRoleOption | undefined = roleOptions.find(
    (role: CreateRoleOption): boolean => role.code === createRequest.primary_role,
  );
  const displayRole = useCallback(
    (roleCode: RoleCode): string =>
      roleOptions.find((role: CreateRoleOption): boolean => role.code === roleCode)?.label ?? roleLabel(roleCode),
    [roleOptions],
  );

  const createMutation = useMutation({
    mutationFn: (variables: CreateAuthUserVariables): Promise<AuthUserSummary> =>
      createAuthUser(variables.request, variables.idempotencyKey),
    onSuccess: (created: AuthUserSummary): void => {
      queryClient.setQueryData<AuthUserSummary[]>(
        authAdminQueryKeys.all,
        (current = []) => [...current, created],
      );
      setFeedback({
        kind: "success",
        message: `Đã tạo tài khoản ${created.username} và xác nhận email.`,
      });
      setCreateRequest(emptyCreateRequest);
      setCreateAttempt(null);
      setShowPassword(false);
      setShowCreate(false);
    },
    onError: (error: Error): void => {
      setFeedback({
        kind: "error",
        message: friendlyApiError(
          error,
          "Không thể tạo tài khoản. Vui lòng kiểm tra thông tin và thử lại.",
        ),
      });
    },
  });

  const statusMutation = useMutation({
    mutationFn: ({
      authUserId,
      disabled,
    }: {
      authUserId: string;
      disabled: boolean;
    }) => setAuthUserStatus(authUserId, { disabled }),
    onSuccess: (updated) => {
      queryClient.setQueryData<AuthUserSummary[]>(
        authAdminQueryKeys.all,
        (current = []) =>
          current.map((user) =>
            user.auth_user_id === updated.auth_user_id ? updated : user,
          ),
      );
      setFeedback({
        kind: "success",
        message: userIsDisabled(updated)
          ? `Đã vô hiệu hóa ${updated.username}.`
          : `Đã kích hoạt lại ${updated.username}.`,
      });
    },
    onError: (error) => {
      setFeedback({
        kind: "error",
        message: friendlyApiError(
          error,
          "Không thể thay đổi trạng thái tài khoản.",
        ),
      });
    },
  });

  const users = useMemo(() => {
    const term = search.trim().toLocaleLowerCase("vi");
    if (!term) {
      return usersQuery.data ?? [];
    }
    return (usersQuery.data ?? []).filter((user) =>
      [user.username, user.email ?? "", displayRole(user.primary_role)]
        .join(" ")
        .toLocaleLowerCase("vi")
        .includes(term),
    );
  }, [displayRole, search, usersQuery.data]);

  if (!canRead) {
    return (
      <section className="panel p-8 text-center">
        <ShieldCheck className="mx-auto size-10 text-slate-400" />
        <h2 className="mt-4 text-lg font-bold text-slate-950">
          Chưa có quyền quản trị tài khoản
        </h2>
        <p className="mt-2 text-sm text-slate-500">
          Vui lòng liên hệ chủ doanh nghiệp để được cấp quyền phù hợp.
        </p>
      </section>
    );
  }

  const submitCreate = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    setFeedback(null);
    const normalizedRequest: CreateAuthUserRequest = {
      ...createRequest,
      username: createRequest.username.trim(),
      email: createRequest.email.trim(),
    };
    const requestSignature: string = JSON.stringify(normalizedRequest);
    const idempotencyKey: string =
      createAttempt?.requestSignature === requestSignature
        ? createAttempt.idempotencyKey
        : crypto.randomUUID();
    setCreateAttempt({ requestSignature, idempotencyKey });
    createMutation.mutate({ request: normalizedRequest, idempotencyKey });
  };

  const toggleStatus = (user: AuthUserSummary): void => {
    const disabled: boolean = userIsDisabled(user);
    if (
      !disabled &&
      !window.confirm(
        `Vô hiệu hóa ${user.username} trong doanh nghiệp này? Danh tính đăng nhập và quyền ở doanh nghiệp khác không bị thay đổi.`,
      )
    ) {
      return;
    }
    setFeedback(null);
    statusMutation.mutate({
      authUserId: user.auth_user_id,
      disabled: !disabled,
    });
  };

  return (
    <div className="space-y-5">
      <section className="panel p-5 sm:p-6">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-center">
          <div className="flex items-start gap-3">
            <div className="grid size-11 shrink-0 place-items-center rounded-xl bg-blue-50 text-blue-700">
              <KeyRound className="size-5" />
            </div>
            <div>
              <h2 className="text-lg font-bold text-slate-950">
                Tài khoản đăng nhập
              </h2>
              <p className="mt-1 max-w-2xl text-sm leading-6 text-slate-500">
                Quản lý tài khoản đăng nhập, trạng thái làm việc và quyền truy cập của từng người trong doanh nghiệp.
              </p>
            </div>
          </div>
          <Link className="action-secondary lg:ml-auto" to="/admin/access-control">
            Phân quyền chi tiết
          </Link>
          {canCreate ? (
            <button
              className="action-primary"
              onClick={() => {
                const initialRole: CreateRoleOption | undefined =
                  grantableRoles.find((role: CreateRoleOption): boolean => role.code === "staff") ??
                  grantableRoles[0];
                setFeedback(null);
                setCreateRequest({
                  ...emptyCreateRequest,
                  primary_role: initialRole?.code ?? "staff",
                  branch_ids:
                    initialRole?.code === "tenant_owner" || !profile?.active_branch_id
                      ? []
                      : [profile.active_branch_id],
                });
                setShowCreate(true);
              }}
              type="button"
            >
              <Plus className="size-4" />
              Tạo tài khoản
            </button>
          ) : null}
        </div>
      </section>

      {feedback ? (
        <div
          className={`flex items-start gap-3 rounded-2xl border px-4 py-3 text-sm font-medium ${
            feedback.kind === "success"
              ? "border-emerald-200 bg-emerald-50 text-emerald-800"
              : "border-red-200 bg-red-50 text-red-800"
          }`}
          role="status"
        >
          {feedback.kind === "success" ? (
            <CheckCircle2 className="mt-0.5 size-5 shrink-0" />
          ) : (
            <CircleAlert className="mt-0.5 size-5 shrink-0" />
          )}
          <span>{feedback.message}</span>
        </div>
      ) : null}

      <section className="panel overflow-hidden">
        <div className="flex flex-col gap-3 border-b border-slate-200 p-4 sm:flex-row sm:items-center sm:justify-between sm:p-5">
          <div className="relative w-full max-w-md">
            <Search className="pointer-events-none absolute left-3.5 top-1/2 size-4 -translate-y-1/2 text-slate-400" />
            <input
              aria-label="Tìm tài khoản"
              className="min-h-11 w-full rounded-xl border border-slate-300 pl-10 pr-4 text-sm outline-none focus:border-blue-500 focus:ring-4 focus:ring-blue-100"
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Tìm theo tên, email hoặc vai trò"
              type="search"
              value={search}
            />
          </div>
          <p className="shrink-0 text-sm font-semibold text-slate-500">
            {users.length} tài khoản
          </p>
        </div>

        {usersQuery.isPending ? (
          <div className="grid min-h-64 place-items-center">
            <div className="text-center text-sm font-medium text-slate-500">
              <LoaderCircle className="mx-auto mb-3 size-6 animate-spin text-blue-600" />
              Đang tải tài khoản...
            </div>
          </div>
        ) : null}

        {usersQuery.error ? (
          <div className="px-6 py-12 text-center">
            <CircleAlert className="mx-auto size-10 text-red-500" />
            <h3 className="mt-4 font-bold text-slate-950">
              Chưa thể tải tài khoản
            </h3>
            <p className="mt-2 text-sm text-slate-500">
              {friendlyApiError(
                usersQuery.error,
                "Máy chủ chưa thể trả về danh sách tài khoản.",
              )}
            </p>
            <button
              className="action-secondary mt-5"
              onClick={() => void usersQuery.refetch()}
              type="button"
            >
              <RefreshCw className="size-4" />
              Thử tải lại
            </button>
          </div>
        ) : null}

        {!usersQuery.isPending && !usersQuery.error && users.length === 0 ? (
          <div className="px-6 py-14 text-center">
            <KeyRound className="mx-auto size-11 text-slate-300" />
            <h3 className="mt-4 font-bold text-slate-950">
              {search ? "Không tìm thấy tài khoản" : "Chưa có tài khoản"}
            </h3>
            <p className="mt-2 text-sm text-slate-500">
              {search
                ? "Hãy thử một từ khóa khác."
                : "Tạo tài khoản đầu tiên cho nhân sự hoặc người quản lý."}
            </p>
          </div>
        ) : null}

        {users.length > 0 ? (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[900px] text-left text-sm">
              <thead className="bg-slate-50 text-xs uppercase tracking-wide text-slate-500">
                <tr>
                  <th className="px-5 py-3 font-bold">Người dùng</th>
                  <th className="px-5 py-3 font-bold">Vai trò</th>
                  <th className="px-5 py-3 font-bold">Trạng thái</th>
                  <th className="px-5 py-3 font-bold">Đăng nhập gần nhất</th>
                  <th className="px-5 py-3 text-right font-bold">Thao tác</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {users.map((user) => {
                  const disabled: boolean = userIsDisabled(user);
                  const isSelf: boolean = user.account_id === profile?.account_id;
                  const isChanging: boolean =
                    statusMutation.isPending &&
                    statusMutation.variables?.authUserId === user.auth_user_id;
                  const providerMissing: boolean = user.provider_status === "missing";
                  const providerDisabled: boolean = user.provider_status === "disabled";
                  const assignedBranchNames: string[] = user.branch_ids.map(
                    (branchId: string): string =>
                      branches.find((branch: BranchSummary): boolean => branch.id === branchId)?.name ?? branchId,
                  );

                  return (
                    <tr className="hover:bg-slate-50/70" key={user.account_id}>
                      <td className="px-5 py-4">
                        <div className="flex items-center gap-3">
                          <div className="grid size-10 shrink-0 place-items-center rounded-full bg-blue-50 font-bold text-blue-700">
                            {user.username.slice(0, 1).toUpperCase()}
                          </div>
                          <div className="min-w-0">
                            <p className="font-bold text-slate-900">
                              {user.username}
                              {isSelf ? (
                                <span className="ml-2 rounded-full bg-violet-50 px-2 py-0.5 text-[10px] font-bold uppercase text-violet-700">
                                  Bạn
                                </span>
                              ) : null}
                            </p>
                            <p className="mt-0.5 truncate text-xs text-slate-500">
                              {user.email ?? "Không tìm thấy email Auth"}
                              {user.email_confirmed ? " · Đã xác nhận" : ""}
                            </p>
                          </div>
                        </div>
                      </td>
                      <td className="px-5 py-4 font-medium text-slate-700">
                        <p>{displayRole(user.primary_role)}</p>
                        <p className="mt-1 text-xs font-normal text-slate-500">
                          {assignedBranchNames.length > 0
                            ? assignedBranchNames.join(", ")
                            : "Toàn doanh nghiệp"}
                        </p>
                      </td>
                      <td className="px-5 py-4">
                        <span
                          className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-bold ${
                            providerMissing
                              ? "bg-amber-50 text-amber-700"
                              : disabled || providerDisabled
                                ? "bg-red-50 text-red-700"
                                : "bg-emerald-50 text-emerald-700"
                          }`}
                        >
                          <span className="size-1.5 rounded-full bg-current" />
                          {providerMissing
                            ? "Thiếu danh tính"
                            : providerDisabled
                              ? "Danh tính bị khóa toàn cục"
                            : disabled
                              ? "Đã vô hiệu trong doanh nghiệp"
                              : "Đang hoạt động"}
                        </span>
                      </td>
                      <td className="px-5 py-4 text-slate-600">
                        {formatOptionalDate(user.last_sign_in_at)}
                      </td>
                      <td className="px-5 py-4 text-right">
                        {canDisable ? (
                          <button
                            className={`inline-flex min-h-9 items-center justify-center gap-2 rounded-lg border px-3 text-xs font-bold transition ${
                              disabled
                                ? "border-emerald-200 text-emerald-700 hover:bg-emerald-50"
                                : "border-red-200 text-red-700 hover:bg-red-50"
                            }`}
                            disabled={
                              isChanging ||
                              providerMissing ||
                              (isSelf && !disabled)
                            }
                            onClick={() => toggleStatus(user)}
                            title={
                              providerDisabled
                                ? "Danh tính đăng nhập đang bị khóa ở cấp nền tảng"
                                : isSelf && !disabled
                                ? "Không thể vô hiệu hóa tài khoản đang sử dụng"
                                : undefined
                            }
                            type="button"
                          >
                            {isChanging ? (
                              <LoaderCircle className="size-4 animate-spin" />
                            ) : disabled ? (
                              <UserRoundCheck className="size-4" />
                            ) : (
                              <UserRoundX className="size-4" />
                            )}
                            {disabled ? "Kích hoạt" : "Vô hiệu hóa"}
                          </button>
                        ) : (
                          <span className="text-xs text-slate-400">Chỉ xem</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        ) : null}
      </section>

      {showCreate ? (
        <div className="fixed inset-0 z-50 grid place-items-center overflow-y-auto bg-slate-950/55 p-4 backdrop-blur-sm">
          <button
            aria-label="Đóng hộp thoại"
            className="absolute inset-0 cursor-default"
            onClick={() => !createMutation.isPending && setShowCreate(false)}
            type="button"
          />
          <section
            aria-labelledby="create-auth-user-title"
            aria-modal="true"
            className="relative my-6 w-full max-w-2xl rounded-3xl bg-white p-6 shadow-2xl sm:p-8"
            role="dialog"
          >
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2
                  className="text-xl font-bold text-slate-950"
                  id="create-auth-user-title"
                >
                  Tạo tài khoản đăng nhập
                </h2>
                <p className="mt-2 text-sm leading-6 text-slate-500">
                  Email được xác nhận ngay trong môi trường quản trị. Người dùng
                  có thể đăng nhập bằng mật khẩu sau khi tạo.
                </p>
              </div>
              <button
                aria-label="Đóng"
                className="grid size-9 shrink-0 place-items-center rounded-lg text-slate-400 hover:bg-slate-100 hover:text-slate-700"
                disabled={createMutation.isPending}
                onClick={() => setShowCreate(false)}
                type="button"
              >
                <X className="size-5" />
              </button>
            </div>

            <form className="mt-8 space-y-6" onSubmit={submitCreate}>
              <div className="grid gap-6 sm:grid-cols-2">
                <label className="block">
                  <span className="block text-sm font-semibold text-slate-700">
                    Tên hiển thị / đăng nhập
                  </span>
                  <input
                    autoFocus
                    className="mt-3 min-h-12 w-full rounded-xl border-slate-300 px-4"
                    maxLength={128}
                    minLength={3}
                    onChange={(event) =>
                      setCreateRequest((current) => ({
                        ...current,
                        username: event.target.value,
                      }))
                    }
                    required
                    value={createRequest.username}
                  />
                </label>
                <label className="block">
                  <span className="block text-sm font-semibold text-slate-700">
                    Email
                  </span>
                  <input
                    autoComplete="off"
                    className="mt-3 min-h-12 w-full rounded-xl border-slate-300 px-4"
                    onChange={(event) =>
                      setCreateRequest((current) => ({
                        ...current,
                        email: event.target.value,
                      }))
                    }
                    required
                    type="email"
                    value={createRequest.email}
                  />
                </label>
              </div>

              <label className="block rounded-2xl border border-slate-200 bg-slate-50/70 p-4 sm:p-5">
                <span className="text-sm font-semibold text-slate-700">
                  Mật khẩu ban đầu <span className="font-normal text-slate-400">(không bắt buộc)</span>
                </span>
                <span className="relative mt-3 block">
                  <input
                    autoComplete="new-password"
                    className="min-h-12 w-full rounded-xl border-slate-300 bg-white px-4 pr-12"
                    minLength={8}
                    onChange={(event) =>
                      setCreateRequest((current) => ({
                        ...current,
                        password: event.target.value,
                      }))
                    }
                    type={showPassword ? "text" : "password"}
                    value={createRequest.password ?? ""}
                  />
                  <button
                    aria-label={showPassword ? "Ẩn mật khẩu" : "Hiện mật khẩu"}
                    className="absolute inset-y-0 right-0 grid w-11 place-items-center text-slate-400 hover:text-slate-700"
                    onClick={() => setShowPassword((visible) => !visible)}
                    type="button"
                  >
                    {showPassword ? (
                      <EyeOff className="size-4" />
                    ) : (
                      <Eye className="size-4" />
                    )}
                  </button>
                </span>
                <span className="mt-2 block text-xs leading-5 text-slate-500">
                  Nhập khi tạo tài khoản mới. Nếu email đã tồn tại, tài khoản hiện có sẽ được liên kết với doanh nghiệp này. Có thể để trống khi người dùng đăng nhập bằng Google hoặc Facebook.
                </span>
              </label>
              <div className="rounded-2xl border border-slate-200 p-4 sm:p-5">
                <div className="grid gap-6 sm:grid-cols-2">
                  <label className="block">
                    <span className="block text-sm font-semibold text-slate-700">
                      Vai trò chính
                    </span>
                    <select
                      className="mt-3 min-h-12 w-full rounded-xl border-slate-300 px-4"
                      disabled={grantableRoles.length === 0}
                      onChange={(event): void => {
                        const primaryRole: RoleCode = event.target.value;
                        setCreateRequest((current: CreateAuthUserRequest): CreateAuthUserRequest => ({
                          ...current,
                          primary_role: primaryRole,
                          branch_ids:
                            primaryRole === "tenant_owner"
                              ? []
                              : primaryRole === "executive_manager"
                                ? current.branch_ids.length > 0
                                  ? current.branch_ids
                                  : profile?.active_branch_id
                                    ? [profile.active_branch_id]
                                    : []
                                : [current.branch_ids[0] ?? profile?.active_branch_id ?? branches[0]?.id].filter(
                                    (branchId: string | undefined): branchId is string => branchId !== undefined,
                                  ),
                        }));
                      }}
                      value={createRequest.primary_role}
                    >
                      {grantableRoles.map((role: CreateRoleOption) => (
                        <option key={role.code} value={role.code}>
                          {role.label}
                        </option>
                      ))}
                    </select>
                  </label>

              {createRequest.primary_role === "tenant_owner" ? (
                <div className="rounded-xl border border-blue-100 bg-blue-50 px-4 py-3 text-sm leading-6 text-blue-800 sm:col-span-1">
                  Chủ doanh nghiệp có quyền toàn tenant, nên không gán trực tiếp vào chi nhánh.
                </div>
              ) : createRequest.primary_role === "executive_manager" ? (
                <fieldset className="sm:col-span-2">
                  <legend className="text-sm font-semibold text-slate-700">Các chi nhánh phụ trách</legend>
                  <div className="mt-3 grid gap-3 sm:grid-cols-2">
                    {branches.map((branch: BranchSummary) => (
                      <label
                        className="flex min-h-11 items-center gap-3 rounded-xl border border-slate-200 px-3 text-sm font-medium text-slate-700"
                        key={branch.id}
                      >
                        <input
                          checked={createRequest.branch_ids.includes(branch.id)}
                          onChange={(event): void => {
                            setCreateRequest((current: CreateAuthUserRequest): CreateAuthUserRequest => ({
                              ...current,
                              branch_ids: event.target.checked
                                ? [...current.branch_ids, branch.id]
                                : current.branch_ids.filter((branchId: string): boolean => branchId !== branch.id),
                            }));
                          }}
                          type="checkbox"
                        />
                        {branch.name}
                      </label>
                    ))}
                  </div>
                </fieldset>
              ) : (
                <label className="block">
                  <span className="block text-sm font-semibold text-slate-700">
                    {selectedRole?.scope === "tenant" ? "Chi nhánh hồ sơ nhân sự" : "Chi nhánh"}
                  </span>
                  <select
                    className="mt-3 min-h-12 w-full rounded-xl border-slate-300 px-4"
                    onChange={(event): void => {
                      setCreateRequest((current: CreateAuthUserRequest): CreateAuthUserRequest => ({
                        ...current,
                        branch_ids: [event.target.value],
                      }));
                    }}
                    required
                    value={createRequest.branch_ids[0] ?? ""}
                  >
                    <option disabled value="">Chọn chi nhánh</option>
                    {branches.map((branch: BranchSummary) => (
                      <option key={branch.id} value={branch.id}>
                        {branch.name}
                      </option>
                    ))}
                  </select>
                </label>
              )}
                </div>
                {selectedRole && !selectedRole.isSystem ? (
                  <p className="mt-4 border-t border-slate-100 pt-4 text-xs leading-5 text-slate-500">
                    Vai trò tùy chỉnh “{selectedRole.label}” sẽ được gán ngay khi tạo tài khoản.
                    {selectedRole.scope === "tenant"
                      ? " Quyền của vai trò áp dụng toàn doanh nghiệp; chi nhánh trên dùng cho hồ sơ nhân sự."
                      : " Quyền của vai trò áp dụng tại chi nhánh đã chọn."}
                  </p>
                ) : null}
              </div>

              <div className="flex flex-col-reverse gap-3 border-t border-slate-200 pt-5 sm:flex-row sm:justify-end">
                <button
                  className="action-secondary"
                  disabled={createMutation.isPending}
                  onClick={() => setShowCreate(false)}
                  type="button"
                >
                  Hủy
                </button>
                <button
                  className="action-primary"
                  disabled={createMutation.isPending}
                  type="submit"
                >
                  {createMutation.isPending ? (
                    <LoaderCircle className="size-4 animate-spin" />
                  ) : (
                    <Plus className="size-4" />
                  )}
                  {createMutation.isPending
                    ? "Đang tạo tài khoản..."
                    : "Tạo tài khoản"}
                </button>
              </div>
            </form>
          </section>
        </div>
      ) : null}
    </div>
  );
}
