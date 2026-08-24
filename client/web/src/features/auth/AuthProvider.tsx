import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { CurrentUserProfile, TenantMembershipSummary } from "../../api/generated/contracts";
import {
  setApiActiveBranchId,
  setApiActiveTenantId,
  setAuthenticationLostHandler,
  setAuthenticationRefreshHandler,
} from "../../shared/api/client";
import {
  logoutSession,
  refreshAccessToken,
  restoreSession,
  selectTenantSession,
  signInWithPassword,
  type ApplicationSessionContext,
} from "./api";

type AuthStatus = "loading" | "authenticated" | "anonymous";

interface AuthContextValue {
  status: AuthStatus;
  profile: CurrentUserProfile | null;
  memberships: TenantMembershipSummary[];
  selectTenant(tenantId: string): Promise<void>;
  selectBranch(branchId: string): void;
  login(email: string, password: string): Promise<void>;
  logout(): Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);
const TENANT_STORAGE_KEY: string = "shepherd.active-tenant";

function branchStorageKey(tenantId: string): string {
  return `shepherd.active-branch.${tenantId}`;
}

function initializeActiveBranch(profile: CurrentUserProfile): CurrentUserProfile {
  const storedBranchId: string | null = localStorage.getItem(branchStorageKey(profile.tenant_id));
  const activeBranchId: string | null =
    storedBranchId !== null && profile.branch_ids.includes(storedBranchId)
      ? storedBranchId
      : profile.active_branch_id;
  setApiActiveBranchId(activeBranchId);
  if (activeBranchId !== null) {
    localStorage.setItem(branchStorageKey(profile.tenant_id), activeBranchId);
  }
  return { ...profile, active_branch_id: activeBranchId };
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AuthStatus>("loading");
  const [profile, setProfile] = useState<CurrentUserProfile | null>(null);
  const [memberships, setMemberships] = useState<TenantMembershipSummary[]>([]);

  useEffect(() => {
    let active: boolean = true;

    const preferredTenantId: string | null = localStorage.getItem(TENANT_STORAGE_KEY);
    void restoreSession(preferredTenantId)
      .then((restoredContext: ApplicationSessionContext) => {
        if (active) {
          localStorage.setItem(TENANT_STORAGE_KEY, restoredContext.profile.tenant_id);
          setMemberships(restoredContext.memberships);
          setProfile(initializeActiveBranch(restoredContext.profile));
          setStatus("authenticated");
        }
      })
      .catch(() => {
        if (active) {
          setProfile(null);
          setMemberships([]);
          setApiActiveTenantId(null);
          setApiActiveBranchId(null);
          setStatus("anonymous");
        }
      });

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    setAuthenticationRefreshHandler(() => refreshAccessToken(true));
    setAuthenticationLostHandler(() => {
      setProfile(null);
      setMemberships([]);
      setApiActiveTenantId(null);
      setApiActiveBranchId(null);
      setStatus("anonymous");
    });

    return () => {
      setAuthenticationLostHandler(null);
      setAuthenticationRefreshHandler(null);
    };
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      status,
      profile,
      memberships,
      async selectTenant(tenantId: string): Promise<void> {
        if (!memberships.some(
          (membership: TenantMembershipSummary): boolean => membership.tenant_id === tenantId,
        )) {
          console.warn("Ignored unauthorized frontend tenant selection", { tenantId });
          return;
        }
        if (profile?.tenant_id === tenantId) {
          return;
        }
        try {
          const selectedProfile: CurrentUserProfile = await selectTenantSession(tenantId);
          localStorage.setItem(TENANT_STORAGE_KEY, tenantId);
          setProfile(initializeActiveBranch(selectedProfile));
          console.info("Shepherd frontend tenant selection changed", {
            tenantId,
            accountId: selectedProfile.account_id,
          });
        } catch (error: unknown) {
          setApiActiveTenantId(profile?.tenant_id ?? null);
          setApiActiveBranchId(profile?.active_branch_id ?? null);
          console.warn("Shepherd frontend restored the previous tenant after a failed switch", {
            tenantId,
            previousTenantId: profile?.tenant_id ?? null,
          });
          throw error;
        }
      },
      selectBranch(branchId: string): void {
        if (!profile || !profile.branch_ids.includes(branchId)) {
          console.warn("Ignored unauthorized frontend branch selection", { branchId });
          return;
        }
        setApiActiveBranchId(branchId);
        localStorage.setItem(branchStorageKey(profile.tenant_id), branchId);
        setProfile({ ...profile, active_branch_id: branchId });
      },
      async login(email: string, password: string): Promise<void> {
        const preferredTenantId: string | null = localStorage.getItem(TENANT_STORAGE_KEY);
        const restoredContext: ApplicationSessionContext = await signInWithPassword(
          email,
          password,
          preferredTenantId,
        );
        localStorage.setItem(TENANT_STORAGE_KEY, restoredContext.profile.tenant_id);
        setMemberships(restoredContext.memberships);
        setProfile(initializeActiveBranch(restoredContext.profile));
        setStatus("authenticated");
      },
      async logout(): Promise<void> {
        try {
          await logoutSession();
        } finally {
          setProfile(null);
          setMemberships([]);
          setApiActiveTenantId(null);
          setApiActiveBranchId(null);
          setStatus("anonymous");
        }
      },
    }),
    [memberships, profile, status],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth phải được dùng bên trong AuthProvider");
  }

  return context;
}
