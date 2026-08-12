import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { AuthProfileResponse, AuthRequest } from "../../api/generated/contracts";
import {
  clearAccessToken,
  setAuthenticationLostHandler,
} from "../../shared/api/client";
import { loginSession, logoutSession, restoreSession } from "./api";

type AuthStatus = "loading" | "authenticated" | "anonymous";

interface AuthContextValue {
  status: AuthStatus;
  profile: AuthProfileResponse | null;
  login(input: AuthRequest): Promise<void>;
  logout(): Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AuthStatus>("loading");
  const [profile, setProfile] = useState<AuthProfileResponse | null>(null);

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
          clearAccessToken();
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
      async login(input) {
        const authenticatedProfile = await loginSession(input);
        setProfile(authenticatedProfile);
        setStatus("authenticated");
      },
      async logout() {
        await logoutSession();
        setProfile(null);
        setStatus("anonymous");
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
