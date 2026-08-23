import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { CurrentUserProfile } from "../../api/generated/contracts";
import {
  setApiActiveBranchId,
  setAuthenticationLostHandler,
  setAuthenticationRefreshHandler,
} from "../../shared/api/client";
import {
  logoutSession,
  refreshAccessToken,
  restoreSession,
  signInWithPassword,
} from "./api";

type AuthStatus = "loading" | "authenticated" | "anonymous";

interface AuthContextValue {
  status: AuthStatus;
  profile: CurrentUserProfile | null;
  selectBranch(branchId: string): void;
  login(email: string, password: string): Promise<void>;
  logout(): Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

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

  useEffect(() => {
    let active = true;

    void restoreSession()
      .then((restoredProfile) => {
        if (active) {
          setProfile(initializeActiveBranch(restoredProfile));
          setStatus("authenticated");
        }
      })
      .catch(() => {
        if (active) {
          setProfile(null);
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
      selectBranch(branchId: string): void {
        if (!profile || !profile.branch_ids.includes(branchId)) {
          console.warn("Ignored unauthorized frontend branch selection", { branchId });
          return;
        }
        setApiActiveBranchId(branchId);
        localStorage.setItem(branchStorageKey(profile.tenant_id), branchId);
        setProfile({ ...profile, active_branch_id: branchId });
      },
      async login(email: string, password: string) {
        const restoredProfile = await signInWithPassword(email, password);
        setProfile(initializeActiveBranch(restoredProfile));
        setStatus("authenticated");
      },
      async logout() {
        try {
          await logoutSession();
        } finally {
          setProfile(null);
          setApiActiveBranchId(null);
          setStatus("anonymous");
        }
      },
    }),
    [profile, status],
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
