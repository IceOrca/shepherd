import AdminApp from "./AdminApp";
import UserApp from "./UserApp";

function normalizePath(pathname: string): string {
  return pathname.replace(/\/+$/, "") || "/";
}

export default function App() {
  const path = normalizePath(window.location.pathname);

  if (path === "/admin" || path.startsWith("/admin/")) {
    return <AdminApp />;
  }

  return <UserApp />;
}
