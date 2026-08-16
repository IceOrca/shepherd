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
  login(email: string, password: string): Promise<void>;
  logout(): Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AuthStatus>("loading");
  const [profile, setProfile] = useState<CurrentUserProfile | null>(null);

  useEffect(() => {
    let active = true;

    void restoreSession()
      .then((restoredProfile) => {
        if (active) {
          setProfile(restoredProfile);
          setStatus("authenticated");
        }
      })
      .catch(() => {
        if (active) {
          setProfile(null);
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
      async login(email: string, password: string) {
        const restoredProfile = await signInWithPassword(email, password);
        setProfile(restoredProfile);
        setStatus("authenticated");
      },
      async logout() {
        try {
          await logoutSession();
        } finally {
          setProfile(null);
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
