import { useInfiniteQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Building2,
  Check,
  ClipboardList,
  KeyRound,
  LoaderCircle,
  Plus,
  RefreshCw,
  Save,
  ShieldCheck,
  Trash2,
  UserRoundCog,
  UsersRound,
} from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import type {
  AccessControlBranch,
  AccessControlPermission,
  AccessControlRole,
  AccessControlSnapshot,
  AccessControlUser,
  AccessRoleScope,
  AccountPermissionOverrideContract,
  AccountRoleAssignmentContract,
  CreateAccessControlBranchRequest,
  CreateAccessControlRoleRequest,
  PermissionCode,
  PermissionOverrideEffect,
  RoleCode,
  UpdateAccessControlBranchRequest,
  UpdateAccessControlRoleRequest,
  UpdateAccountAccessRequest,
} from "../../api/generated/contracts";
import { friendlyApiError } from "../../shared/api/client";
import { CursorPagination } from "../../shared/components/CursorPagination";
import { roleLabel } from "../../shared/lib/format";
import {
  authAdminQueryKeys,
  createAccessControlBranch,
  createAccessControlRole,
  getAccessControlSnapshot,
  updateAccessControlBranch,
  updateAccessControlRole,
  updateAccountAccess,
} from "./api";

type AccessTab = "branches" | "roles" | "users" | "audit";

interface BranchEditor {
  id: string;
  name: string;
  time_zone: string;
  status: string;
  version: number;
}

interface RoleEditor {
  code: RoleCode;
  display_name: string;
  description: string;
  is_active: boolean;
  version: number;
  permission_codes: PermissionCode[];
}

interface UserAccessEditor {
  account_id: string;
  primary_role: RoleCode;
  expected_version: number;
  assignments: AccountRoleAssignmentContract[];
  permission_overrides: AccountPermissionOverrideContract[];
}

interface Feedback {
  kind: "success" | "error";
  message: string;
}

const emptyBranchRequest: CreateAccessControlBranchRequest = {
  code: "",
  name: "",
  time_zone: "Asia/Ho_Chi_Minh",
};

const emptyRoleRequest: CreateAccessControlRoleRequest = {
  code: "",
  display_name: "",
  description: null,
  scope: "branch",
  permission_codes: [],
};

function branchName(branches: AccessControlBranch[], branchId: string | null): string {
  if (branchId === null) {
    return "Toàn doanh nghiệp";
  }
  return branches.find((branch: AccessControlBranch): boolean => branch.id === branchId)?.name ?? branchId;
}

function roleName(roles: AccessControlRole[], roleCode: RoleCode): string {
  return roles.find((role: AccessControlRole): boolean => role.code === roleCode)?.display_name ?? roleLabel(roleCode);
}

function permissionName(permissions: AccessControlPermission[], permissionCode: PermissionCode): string {
  return permissions.find(
    (permission: AccessControlPermission): boolean => permission.code === permissionCode,
  )?.display_name ?? "Quyền không còn trong danh mục";
}

function roleScopeLabel(scope: AccessRoleScope | undefined): string {
  return scope === "tenant" ? "Toàn doanh nghiệp" : "Theo chi nhánh";
}

function newUserEditor(user: AccessControlUser): UserAccessEditor {
  return {
    account_id: user.account_id,
    primary_role: user.primary_role,
    expected_version: user.authorization_version,
    assignments: user.assignments.map(
      (assignment: AccountRoleAssignmentContract): AccountRoleAssignmentContract => ({ ...assignment }),
    ),
    permission_overrides: user.permission_overrides.map(
      (accountOverride: AccountPermissionOverrideContract): AccountPermissionOverrideContract => ({
        ...accountOverride,
      }),
    ),
  };
}

export function AccessControlPage() {
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<AccessTab>("users");
  const [feedback, setFeedback] = useState<Feedback | null>(null);
  const [branchRequest, setBranchRequest] =
    useState<CreateAccessControlBranchRequest>(emptyBranchRequest);
  const [branchEditor, setBranchEditor] = useState<BranchEditor | null>(null);
  const [roleRequest, setRoleRequest] =
    useState<CreateAccessControlRoleRequest>(emptyRoleRequest);
  const [selectedRoleCode, setSelectedRoleCode] = useState<RoleCode>("");
  const [roleEditor, setRoleEditor] = useState<RoleEditor | null>(null);
  const [selectedUserId, setSelectedUserId] = useState<string>("");
  const [userEditor, setUserEditor] = useState<UserAccessEditor | null>(null);
  const [assignmentRoleCode, setAssignmentRoleCode] = useState<RoleCode>("");
  const [assignmentBranchId, setAssignmentBranchId] = useState<string>("");
  const [overridePermissionCode, setOverridePermissionCode] = useState<PermissionCode>("");
  const [overrideBranchId, setOverrideBranchId] = useState<string>("");
  const [overrideEffect, setOverrideEffect] = useState<PermissionOverrideEffect>("allow");
  const [rolePage, setRolePage] = useState<number>(1);
  const [userPage, setUserPage] = useState<number>(1);
  const [auditPage, setAuditPage] = useState<number>(1);

  const snapshotQuery = useInfiniteQuery({
    queryKey: [...authAdminQueryKeys.accessControl, "roles"],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }: { pageParam: string | null }): Promise<AccessControlSnapshot> =>
      getAccessControlSnapshot({ roleCursor: pageParam }),
    getNextPageParam: (lastPage: AccessControlSnapshot): string | undefined =>
      lastPage.role_next_cursor ?? undefined,
  });
  const userQuery = useInfiniteQuery({
    queryKey: [...authAdminQueryKeys.accessControl, "users"],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }: { pageParam: string | null }): Promise<AccessControlSnapshot> =>
      getAccessControlSnapshot({ userCursor: pageParam }),
    getNextPageParam: (lastPage: AccessControlSnapshot): string | undefined =>
      lastPage.user_next_cursor ?? undefined,
  });
  const auditQuery = useInfiniteQuery({
    queryKey: [...authAdminQueryKeys.accessControl, "audit"],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }: { pageParam: string | null }): Promise<AccessControlSnapshot> =>
      getAccessControlSnapshot({ auditCursor: pageParam }),
    getNextPageParam: (lastPage: AccessControlSnapshot): string | undefined =>
      lastPage.audit_next_cursor ?? undefined,
  });
  const rolePages: AccessControlSnapshot[] = snapshotQuery.data?.pages ?? [];
  const userPages: AccessControlSnapshot[] = userQuery.data?.pages ?? [];
  const auditPages: AccessControlSnapshot[] = auditQuery.data?.pages ?? [];
  const snapshot: AccessControlSnapshot | undefined = rolePages[0];
  const branches: AccessControlBranch[] = snapshot?.branches ?? [];
  const permissions: AccessControlPermission[] = snapshot?.permissions ?? [];
  const roles: AccessControlRole[] = rolePages[rolePage - 1]?.roles ?? [];
  const allLoadedRoles: AccessControlRole[] = rolePages.flatMap(
    (page: AccessControlSnapshot): AccessControlRole[] => page.roles,
  );
  const users: AccessControlUser[] = userPages[userPage - 1]?.users ?? [];
  const auditEntries = auditPages[auditPage - 1]?.audit ?? [];
  const activeRoles: AccessControlRole[] = allLoadedRoles.filter(
    (role: AccessControlRole): boolean => role.is_active,
  );

  const changeRolePage = (nextPage: number): void => {
    if (nextPage < 1) return;
    if (nextPage <= rolePages.length) return setRolePage(nextPage);
    if (nextPage === rolePages.length + 1 && snapshotQuery.hasNextPage) {
      void snapshotQuery.fetchNextPage().then((result): void => {
        if ((result.data?.pages.length ?? 0) >= nextPage) setRolePage(nextPage);
      });
    }
  };
  const changeUserPage = (nextPage: number): void => {
    if (nextPage < 1) return;
    if (nextPage <= userPages.length) return setUserPage(nextPage);
    if (nextPage === userPages.length + 1 && userQuery.hasNextPage) {
      void userQuery.fetchNextPage().then((result): void => {
        if ((result.data?.pages.length ?? 0) >= nextPage) setUserPage(nextPage);
      });
    }
  };
  const changeAuditPage = (nextPage: number): void => {
    if (nextPage < 1) return;
    if (nextPage <= auditPages.length) return setAuditPage(nextPage);
    if (nextPage === auditPages.length + 1 && auditQuery.hasNextPage) {
      void auditQuery.fetchNextPage().then((result): void => {
        if ((result.data?.pages.length ?? 0) >= nextPage) setAuditPage(nextPage);
      });
    }
  };
  useEffect((): void => {
    if (roles.length > 0 && !roles.some((role: AccessControlRole): boolean => role.code === selectedRoleCode)) {
      setSelectedRoleCode(roles[0]?.code ?? "");
    }
  }, [roles, selectedRoleCode]);

  useEffect((): void => {
    const selectedRole: AccessControlRole | undefined = roles.find(
      (role: AccessControlRole): boolean => role.code === selectedRoleCode,
    );
    setRoleEditor(
      selectedRole
        ? {
            code: selectedRole.code,
            display_name: selectedRole.display_name,
            description: selectedRole.description ?? "",
            is_active: selectedRole.is_active,
            version: selectedRole.version,
            permission_codes: [...selectedRole.permission_codes],
          }
        : null,
    );
  }, [roles, selectedRoleCode]);

  useEffect((): void => {
    if (users.length > 0 && !users.some((user: AccessControlUser): boolean => user.account_id === selectedUserId)) {
      setSelectedUserId(users[0]?.account_id ?? "");
    }
  }, [selectedUserId, users]);

  useEffect((): void => {
    const selectedUser: AccessControlUser | undefined = users.find(
      (user: AccessControlUser): boolean => user.account_id === selectedUserId,
    );
    setUserEditor(selectedUser ? newUserEditor(selectedUser) : null);
  }, [selectedUserId, users]);

  const refreshSnapshot = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: authAdminQueryKeys.accessControl });
    await queryClient.invalidateQueries({ queryKey: authAdminQueryKeys.all });
  };

  const branchCreateMutation = useMutation({
    mutationFn: createAccessControlBranch,
    onSuccess: async (created: AccessControlBranch): Promise<void> => {
      setFeedback({ kind: "success", message: `Đã tạo chi nhánh ${created.name}.` });
      setBranchRequest(emptyBranchRequest);
      await refreshSnapshot();
    },
    onError: (mutationError: Error): void =>
      setFeedback({ kind: "error", message: friendlyApiError(mutationError, "Không thể tạo chi nhánh.") }),
  });

  const branchUpdateMutation = useMutation({
    mutationFn: (editor: BranchEditor): Promise<AccessControlBranch> => {
      const request: UpdateAccessControlBranchRequest = {
        name: editor.name,
        time_zone: editor.time_zone,
        status: editor.status,
        expected_version: editor.version,
      };
      return updateAccessControlBranch(editor.id, request);
    },
    onSuccess: async (updated: AccessControlBranch): Promise<void> => {
      setFeedback({ kind: "success", message: `Đã cập nhật chi nhánh ${updated.name}.` });
      setBranchEditor(null);
      await refreshSnapshot();
    },
    onError: (mutationError: Error): void =>
      setFeedback({ kind: "error", message: friendlyApiError(mutationError, "Không thể cập nhật chi nhánh.") }),
  });

  const roleCreateMutation = useMutation({
    mutationFn: createAccessControlRole,
    onSuccess: async (created: AccessControlRole): Promise<void> => {
      setFeedback({ kind: "success", message: `Đã tạo vai trò ${created.display_name}.` });
      setRoleRequest(emptyRoleRequest);
      setSelectedRoleCode(created.code);
      await refreshSnapshot();
    },
    onError: (mutationError: Error): void =>
      setFeedback({ kind: "error", message: friendlyApiError(mutationError, "Không thể tạo vai trò.") }),
  });

  const roleUpdateMutation = useMutation({
    mutationFn: (editor: RoleEditor): Promise<AccessControlRole> => {
      const request: UpdateAccessControlRoleRequest = {
        display_name: editor.display_name,
        description: editor.description.trim() === "" ? null : editor.description,
        is_active: editor.is_active,
        expected_version: editor.version,
        permission_codes: editor.permission_codes,
      };
      return updateAccessControlRole(editor.code, request);
    },
    onSuccess: async (updated: AccessControlRole): Promise<void> => {
      setFeedback({ kind: "success", message: `Đã lưu quyền của vai trò ${updated.display_name}.` });
      await refreshSnapshot();
    },
    onError: (mutationError: Error): void =>
      setFeedback({ kind: "error", message: friendlyApiError(mutationError, "Không thể cập nhật vai trò.") }),
  });

  const userUpdateMutation = useMutation({
    mutationFn: (editor: UserAccessEditor): Promise<AccessControlUser> => {
      const request: UpdateAccountAccessRequest = {
        primary_role: editor.primary_role,
        expected_version: editor.expected_version,
        assignments: editor.assignments,
        permission_overrides: editor.permission_overrides,
      };
      return updateAccountAccess(editor.account_id, request);
    },
    onSuccess: async (updated: AccessControlUser): Promise<void> => {
      setFeedback({ kind: "success", message: `Đã cập nhật phạm vi và quyền của ${updated.username}.` });
      await refreshSnapshot();
    },
    onError: (mutationError: Error): void =>
      setFeedback({ kind: "error", message: friendlyApiError(mutationError, "Không thể cập nhật quyền người dùng.") }),
  });

  const submitBranch = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    branchCreateMutation.mutate(branchRequest);
  };

  const submitRole = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    roleCreateMutation.mutate(roleRequest);
  };

  const toggleRolePermission = (permissionCode: PermissionCode): void => {
    setRoleEditor((current: RoleEditor | null): RoleEditor | null => {
      if (current === null) return null;
      const selected: boolean = current.permission_codes.includes(permissionCode);
      return {
        ...current,
        permission_codes: selected
          ? current.permission_codes.filter((code: PermissionCode): boolean => code !== permissionCode)
          : [...current.permission_codes, permissionCode],
      };
    });
  };

  const addAssignment = (): void => {
    if (userEditor === null || assignmentRoleCode === "") return;
    const role: AccessControlRole | undefined = activeRoles.find(
      (candidate: AccessControlRole): boolean => candidate.code === assignmentRoleCode,
    );
    if (role === undefined) return;
    const branchId: string | null = role.scope === "tenant" ? null : assignmentBranchId || null;
    if (role.scope === "branch" && branchId === null) {
      setFeedback({ kind: "error", message: "Hãy chọn chi nhánh cho vai trò này." });
      return;
    }
    const exists: boolean = userEditor.assignments.some(
      (assignment: AccountRoleAssignmentContract): boolean =>
        assignment.role_code === role.code && assignment.branch_id === branchId,
    );
    if (!exists) {
      setUserEditor({
        ...userEditor,
        assignments: [...userEditor.assignments, { role_code: role.code, branch_id: branchId }],
      });
    }
  };

  const addOverride = (): void => {
    if (userEditor === null || overridePermissionCode === "") return;
    const branchId: string | null = overrideBranchId === "" ? null : overrideBranchId;
    const nextOverride: AccountPermissionOverrideContract = {
      permission_code: overridePermissionCode,
      branch_id: branchId,
      effect: overrideEffect,
      expires_at: null,
    };
    const remaining: AccountPermissionOverrideContract[] = userEditor.permission_overrides.filter(
      (current: AccountPermissionOverrideContract): boolean =>
        current.permission_code !== nextOverride.permission_code || current.branch_id !== nextOverride.branch_id,
    );
    setUserEditor({ ...userEditor, permission_overrides: [...remaining, nextOverride] });
  };

  if (snapshotQuery.isPending) {
    return <div className="grid min-h-72 place-items-center text-sm font-semibold text-slate-500"><LoaderCircle className="mr-2 size-5 animate-spin" />Đang tải cấu hình phân quyền...</div>;
  }

  if (snapshotQuery.error || snapshot === undefined) {
    return (
      <div className="surface-card p-8 text-center">
        <ShieldCheck className="mx-auto size-10 text-red-500" />
        <h2 className="mt-4 text-lg font-bold">Không thể tải cấu hình phân quyền</h2>
        <p className="mt-2 text-sm text-slate-500">{friendlyApiError(snapshotQuery.error, "Máy chủ chưa thể trả dữ liệu quản trị.")}</p>
        <button className="action-secondary mt-5" onClick={() => void snapshotQuery.refetch()} type="button"><RefreshCw className="size-4" />Thử lại</button>
      </div>
    );
  }

  const tabs: Array<{ code: AccessTab; label: string; icon: typeof Building2 }> = [
    { code: "users", label: "Người dùng & phạm vi", icon: UsersRound },
    { code: "roles", label: "Vai trò & quyền", icon: ShieldCheck },
    { code: "branches", label: "Chi nhánh", icon: Building2 },
    { code: "audit", label: "Nhật ký", icon: ClipboardList },
  ];

  return (
    <div className="space-y-6">
      <section className="surface-card p-5 sm:p-6">
        <div className="flex flex-col justify-between gap-4 lg:flex-row lg:items-center">
          <div>
            <p className="text-xs font-bold uppercase tracking-[0.18em] text-blue-600">Quản lý truy cập</p>
            <h1 className="mt-2 text-2xl font-bold text-slate-950">Phân quyền doanh nghiệp</h1>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-500">Vai trò và quyền thuộc riêng từng doanh nghiệp. Quyền theo chi nhánh chỉ có hiệu lực khi người dùng chọn đúng chi nhánh đó.</p>
          </div>
          <Link className="action-secondary" to="/admin/auth-users"><UserRoundCog className="size-4" />Tạo / khóa tài khoản</Link>
        </div>
        <div className="mt-5 flex flex-wrap gap-2">
          {tabs.map((item): React.ReactNode => {
            const Icon: typeof Building2 = item.icon;
            return <button className={`inline-flex min-h-10 items-center gap-2 rounded-xl px-4 text-sm font-bold ${tab === item.code ? "bg-blue-600 text-white" : "bg-slate-100 text-slate-600 hover:bg-slate-200"}`} key={item.code} onClick={(): void => setTab(item.code)} type="button"><Icon className="size-4" />{item.label}</button>;
          })}
        </div>
      </section>

      {feedback ? <div className={`rounded-2xl border px-4 py-3 text-sm font-semibold ${feedback.kind === "success" ? "border-emerald-200 bg-emerald-50 text-emerald-800" : "border-red-200 bg-red-50 text-red-800"}`}>{feedback.message}</div> : null}

      {tab === "branches" ? (
        <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_380px]">
          <section className="surface-card overflow-hidden">
            <div className="border-b border-slate-100 px-5 py-4"><h2 className="font-bold text-slate-950">Danh sách chi nhánh</h2></div>
            <div className="divide-y divide-slate-100">
              {branches.map((branch: AccessControlBranch): React.ReactNode => (
                <button className="flex w-full items-center justify-between gap-4 px-5 py-4 text-left hover:bg-slate-50" key={branch.id} onClick={(): void => setBranchEditor({ id: branch.id, name: branch.name, time_zone: branch.time_zone, status: branch.status, version: branch.version })} type="button">
                  <span><span className="block font-bold text-slate-900">{branch.name}</span><span className="mt-1 block text-xs text-slate-500">{branch.code} · {branch.time_zone}</span></span>
                  <span className={`rounded-full px-2.5 py-1 text-xs font-bold ${branch.status === "active" ? "bg-emerald-50 text-emerald-700" : "bg-slate-100 text-slate-500"}`}>{branch.status === "active" ? "Hoạt động" : "Đã tắt"}</span>
                </button>
              ))}
            </div>
          </section>
          <div className="space-y-6">
            <form className="surface-card space-y-4 p-5" onSubmit={submitBranch}>
              <h2 className="font-bold text-slate-950">Tạo chi nhánh</h2>
              <input className="min-h-11 w-full rounded-xl border-slate-300" placeholder="Mã chi nhánh" required value={branchRequest.code} onChange={(event): void => setBranchRequest({ ...branchRequest, code: event.target.value })} />
              <input className="min-h-11 w-full rounded-xl border-slate-300" placeholder="Tên chi nhánh" required value={branchRequest.name} onChange={(event): void => setBranchRequest({ ...branchRequest, name: event.target.value })} />
              <input className="min-h-11 w-full rounded-xl border-slate-300" placeholder="Múi giờ IANA" required value={branchRequest.time_zone} onChange={(event): void => setBranchRequest({ ...branchRequest, time_zone: event.target.value })} />
              <button className="action-primary w-full" disabled={branchCreateMutation.isPending} type="submit"><Plus className="size-4" />Tạo chi nhánh</button>
            </form>
            {branchEditor ? <form className="surface-card space-y-4 p-5" onSubmit={(event: FormEvent<HTMLFormElement>): void => { event.preventDefault(); branchUpdateMutation.mutate(branchEditor); }}><h2 className="font-bold text-slate-950">Cập nhật chi nhánh</h2><input className="min-h-11 rounded-xl border-slate-300" value={branchEditor.name} onChange={(event): void => setBranchEditor({ ...branchEditor, name: event.target.value })} /><input className="min-h-11 rounded-xl border-slate-300" value={branchEditor.time_zone} onChange={(event): void => setBranchEditor({ ...branchEditor, time_zone: event.target.value })} /><select className="min-h-11 rounded-xl border-slate-300" value={branchEditor.status} onChange={(event): void => setBranchEditor({ ...branchEditor, status: event.target.value })}><option value="active">Hoạt động</option><option value="disabled">Vô hiệu hóa</option></select><button className="action-primary w-full" disabled={branchUpdateMutation.isPending} type="submit"><Save className="size-4" />Lưu chi nhánh</button></form> : null}
          </div>
        </div>
      ) : null}

      {tab === "roles" ? (
        <div className="grid gap-6 xl:grid-cols-[300px_minmax(0,1fr)]">
          <div className="space-y-6">
            <section className="surface-card overflow-hidden">
              <div className="border-b border-slate-100 px-5 py-4 font-bold">Vai trò</div>
              {roles.map((role: AccessControlRole): React.ReactNode => (
                <button className={`block w-full border-b border-slate-100 px-5 py-3 text-left ${selectedRoleCode === role.code ? "bg-blue-50 text-blue-800" : "hover:bg-slate-50"}`} key={role.code} onClick={(): void => setSelectedRoleCode(role.code)} type="button">
                  <span className="block font-bold">{role.display_name}</span>
                  <span className="mt-1 block text-xs opacity-70">{roleScopeLabel(role.scope)} · {role.assigned_account_count} người</span>
                </button>
              ))}
              <CursorPagination currentItemCount={roles.length} currentPage={rolePage} hasNextPage={rolePage < rolePages.length || snapshotQuery.hasNextPage} nextPagePending={snapshotQuery.isFetchingNextPage} onPageChange={changeRolePage} />
            </section>
            <form className="surface-card space-y-3 p-5" onSubmit={submitRole}>
              <h2 className="font-bold">Tạo vai trò tùy chỉnh</h2>
              <input className="min-h-10 w-full rounded-xl border-slate-300" placeholder="Mã vai trò" required value={roleRequest.code} onChange={(event): void => setRoleRequest({ ...roleRequest, code: event.target.value })} />
              <input className="min-h-10 w-full rounded-xl border-slate-300" placeholder="Tên vai trò" required value={roleRequest.display_name} onChange={(event): void => setRoleRequest({ ...roleRequest, display_name: event.target.value })} />
              <select className="min-h-10 w-full rounded-xl border-slate-300" value={roleRequest.scope} onChange={(event): void => setRoleRequest({ ...roleRequest, scope: event.target.value as AccessRoleScope })}>
                <option value="branch">Theo chi nhánh</option>
                <option value="tenant">Toàn doanh nghiệp</option>
              </select>
              <button className="action-primary w-full" type="submit"><Plus className="size-4" />Tạo vai trò</button>
            </form>
          </div>
          {roleEditor ? (
            <section className="surface-card min-w-0 p-5 sm:p-6">
              <div className="grid min-w-0 gap-4 lg:grid-cols-2">
                <label className="block min-w-0">
                  <span className="text-sm font-bold">Tên vai trò</span>
                  <input className="mt-2 min-h-11 w-full min-w-0 rounded-xl border-slate-300" value={roleEditor.display_name} onChange={(event): void => setRoleEditor({ ...roleEditor, display_name: event.target.value })} />
                </label>
                <label className="block min-w-0">
                  <span className="text-sm font-bold">Mã và phạm vi</span>
                  <div className="mt-2 min-h-11 break-words rounded-xl bg-slate-100 px-3 py-3 text-sm">{roleEditor.code} · {roleScopeLabel(roles.find((role: AccessControlRole): boolean => role.code === roleEditor.code)?.scope)}</div>
                </label>
              </div>
              <label className="mt-4 block min-w-0">
                <span className="text-sm font-bold">Mô tả</span>
                <textarea className="mt-2 min-h-24 w-full min-w-0 resize-y rounded-xl border-slate-300" value={roleEditor.description} onChange={(event): void => setRoleEditor({ ...roleEditor, description: event.target.value })} />
              </label>
              <div className="mt-6 grid gap-2 md:grid-cols-2">{permissions.map((permission: AccessControlPermission): React.ReactNode => <label className="flex min-w-0 items-start gap-3 rounded-xl border border-slate-200 p-3 text-sm" key={permission.code}><input checked={roleEditor.permission_codes.includes(permission.code)} className="mt-1" onChange={(): void => toggleRolePermission(permission.code)} type="checkbox" /><span className="min-w-0"><span className="block font-semibold text-slate-800">{permission.display_name}</span><span className="mt-1 block text-xs leading-5 text-slate-500">{permission.description}</span></span></label>)}</div>
              <div className="mt-6 flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between"><label className="flex items-center gap-2 text-sm font-semibold"><input checked={roleEditor.is_active} disabled={roles.find((role: AccessControlRole): boolean => role.code === roleEditor.code)?.is_system} onChange={(event): void => setRoleEditor({ ...roleEditor, is_active: event.target.checked })} type="checkbox" />Đang hoạt động</label><button className="action-primary" disabled={roleUpdateMutation.isPending} onClick={(): void => roleUpdateMutation.mutate(roleEditor)} type="button"><Save className="size-4" />Lưu vai trò và quyền</button></div>
            </section>
          ) : null}
        </div>
      ) : null}

      {tab === "users" && userEditor ? (
        <div className="grid gap-6 xl:grid-cols-[300px_minmax(0,1fr)]">
          <section className="surface-card max-h-[760px] overflow-y-auto">{users.map((user: AccessControlUser): React.ReactNode => <button className={`block w-full border-b border-slate-100 px-5 py-4 text-left ${selectedUserId === user.account_id ? "bg-blue-50" : "hover:bg-slate-50"}`} key={user.account_id} onClick={(): void => setSelectedUserId(user.account_id)} type="button"><span className="block font-bold text-slate-900">{user.username}</span><span className="mt-1 block truncate text-xs text-slate-500">{user.email ?? "Không có email"} · {roleName(allLoadedRoles, user.primary_role)}</span></button>)}<CursorPagination currentItemCount={users.length} currentPage={userPage} hasNextPage={userPage < userPages.length || userQuery.hasNextPage} nextPagePending={userQuery.isFetchingNextPage} onPageChange={changeUserPage} /></section>
          <section className="surface-card p-5 sm:p-6"><div className="flex flex-col justify-between gap-3 sm:flex-row sm:items-center"><div><h2 className="text-lg font-bold">{users.find((user: AccessControlUser): boolean => user.account_id === userEditor.account_id)?.username}</h2><p className="mt-1 text-sm text-slate-500">Gán vai trò trên toàn doanh nghiệp hoặc theo từng chi nhánh; quyền từ chi nhánh khác không được cộng vào.</p></div><select className="min-h-10 rounded-xl border-slate-300" value={userEditor.primary_role} onChange={(event): void => setUserEditor({ ...userEditor, primary_role: event.target.value })}>{activeRoles.map((role: AccessControlRole): React.ReactNode => <option key={role.code} value={role.code}>{role.display_name}</option>)}</select></div>
            <div className="mt-6"><h3 className="font-bold">Vai trò được gán</h3><div className="mt-3 space-y-2">{userEditor.assignments.map((assignment: AccountRoleAssignmentContract, index: number): React.ReactNode => <div className="flex items-center justify-between gap-3 rounded-xl border border-slate-200 px-4 py-3" key={`${assignment.role_code}-${assignment.branch_id}-${index}`}><span className="text-sm"><span className="font-bold">{roleName(roles, assignment.role_code)}</span><span className="ml-2 text-slate-500">{branchName(branches, assignment.branch_id)}</span></span><button aria-label="Xóa vai trò" className="text-red-500" onClick={(): void => setUserEditor({ ...userEditor, assignments: userEditor.assignments.filter((_item: AccountRoleAssignmentContract, itemIndex: number): boolean => itemIndex !== index) })} type="button"><Trash2 className="size-4" /></button></div>)}</div><div className="mt-3 grid gap-2 sm:grid-cols-[1fr_1fr_auto]"><select className="min-h-10 rounded-xl border-slate-300" value={assignmentRoleCode} onChange={(event): void => setAssignmentRoleCode(event.target.value)}><option value="">Chọn vai trò</option>{activeRoles.map((role: AccessControlRole): React.ReactNode => <option key={role.code} value={role.code}>{role.display_name}</option>)}</select><select className="min-h-10 rounded-xl border-slate-300" value={assignmentBranchId} onChange={(event): void => setAssignmentBranchId(event.target.value)}><option value="">Toàn doanh nghiệp / chọn chi nhánh</option>{branches.filter((branch: AccessControlBranch): boolean => branch.status === "active").map((branch: AccessControlBranch): React.ReactNode => <option key={branch.id} value={branch.id}>{branch.name}</option>)}</select><button className="action-secondary" onClick={addAssignment} type="button"><Plus className="size-4" />Thêm</button></div></div>
            <div className="mt-8"><h3 className="font-bold">Ngoại lệ quyền cá nhân</h3><p className="mt-1 text-xs leading-5 text-slate-500">Từ chối luôn được ưu tiên hơn Cho phép. Để trống chi nhánh nếu ngoại lệ áp dụng toàn doanh nghiệp.</p><div className="mt-3 space-y-2">{userEditor.permission_overrides.map((accountOverride: AccountPermissionOverrideContract, index: number): React.ReactNode => <div className="flex items-center justify-between gap-3 rounded-xl border border-slate-200 px-4 py-3" key={`${accountOverride.permission_code}-${accountOverride.branch_id}-${index}`}><span className="text-sm"><span className={`mr-2 rounded px-2 py-0.5 text-xs font-bold ${accountOverride.effect === "allow" ? "bg-emerald-50 text-emerald-700" : "bg-red-50 text-red-700"}`}>{accountOverride.effect === "allow" ? "CHO PHÉP" : "TỪ CHỐI"}</span><span className="font-semibold">{permissionName(permissions, accountOverride.permission_code)}</span><span className="ml-2 text-slate-500">{branchName(branches, accountOverride.branch_id)}</span></span><button className="text-red-500" onClick={(): void => setUserEditor({ ...userEditor, permission_overrides: userEditor.permission_overrides.filter((_item: AccountPermissionOverrideContract, itemIndex: number): boolean => itemIndex !== index) })} type="button"><Trash2 className="size-4" /></button></div>)}</div><div className="mt-3 grid gap-2 lg:grid-cols-[1.4fr_1fr_120px_auto]"><select className="min-h-10 rounded-xl border-slate-300" value={overridePermissionCode} onChange={(event): void => setOverridePermissionCode(event.target.value)}><option value="">Chọn quyền</option>{permissions.map((permission: AccessControlPermission): React.ReactNode => <option key={permission.code} value={permission.code}>{permission.display_name}</option>)}</select><select className="min-h-10 rounded-xl border-slate-300" value={overrideBranchId} onChange={(event): void => setOverrideBranchId(event.target.value)}><option value="">Toàn doanh nghiệp</option>{branches.filter((branch: AccessControlBranch): boolean => branch.status === "active").map((branch: AccessControlBranch): React.ReactNode => <option key={branch.id} value={branch.id}>{branch.name}</option>)}</select><select className="min-h-10 rounded-xl border-slate-300" value={overrideEffect} onChange={(event): void => setOverrideEffect(event.target.value as PermissionOverrideEffect)}><option value="allow">Cho phép</option><option value="deny">Từ chối</option></select><button className="action-secondary" onClick={addOverride} type="button"><Plus className="size-4" />Thêm</button></div></div>
            <div className="mt-8 flex justify-end"><button className="action-primary" disabled={userUpdateMutation.isPending} onClick={(): void => userUpdateMutation.mutate(userEditor)} type="button">{userUpdateMutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : <Save className="size-4" />}Lưu phạm vi và quyền</button></div>
          </section>
        </div>
      ) : null}

      {tab === "audit" ? <section className="surface-card overflow-x-auto"><table className="w-full min-w-[760px] text-left text-sm"><thead className="bg-slate-50 text-xs uppercase tracking-wide text-slate-500"><tr><th className="px-5 py-3">Thời gian</th><th className="px-5 py-3">Hành động</th><th className="px-5 py-3">Đối tượng</th><th className="px-5 py-3">Người thực hiện</th></tr></thead><tbody className="divide-y divide-slate-100">{auditEntries.map((entry): React.ReactNode => <tr key={entry.id}><td className="px-5 py-4 text-slate-500">{new Intl.DateTimeFormat("vi-VN", { dateStyle: "short", timeStyle: "short" }).format(new Date(entry.created_at))}</td><td className="px-5 py-4 font-semibold">{entry.action}</td><td className="px-5 py-4">{entry.object_type} · {entry.object_id}</td><td className="px-5 py-4 text-slate-500">{users.find((user: AccessControlUser): boolean => user.account_id === entry.actor_account_id)?.username ?? entry.actor_account_id}</td></tr>)}</tbody></table>{auditEntries.length === 0 ? <div className="p-10 text-center text-sm text-slate-500"><KeyRound className="mx-auto mb-3 size-8 text-slate-300" />Chưa có thay đổi phân quyền nào.</div> : null}<CursorPagination currentItemCount={auditEntries.length} currentPage={auditPage} hasNextPage={auditPage < auditPages.length || auditQuery.hasNextPage} nextPagePending={auditQuery.isFetchingNextPage} onPageChange={changeAuditPage} /></section> : null}
    </div>
  );
}
