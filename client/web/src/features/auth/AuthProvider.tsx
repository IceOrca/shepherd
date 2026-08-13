import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { CurrentUserProfile } from "../../api/generated/contracts";
import { setAuthenticationLostHandler } from "../../shared/api/client";
import { beginLogin, logoutSession, restoreSession } from "./api";

type AuthStatus = "loading" | "authenticated" | "anonymous";

interface AuthContextValue {
  status: AuthStatus;
  profile: CurrentUserProfile | null;
  login(returnTo?: string): void;
  logout(): void;
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
    setAuthenticationLostHandler(() => {
      setProfile(null);
      setStatus("anonymous");
    });

    return () => setAuthenticationLostHandler(null);
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      status,
      profile,
      login: beginLogin,
      logout: logoutSession,
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
