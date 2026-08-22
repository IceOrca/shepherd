import {
  BriefcaseBusiness,
  Building2,
  CalendarClock,
  ChevronRight,
  CircleUserRound,
  LayoutDashboard,
  LogOut,
  Menu,
  PanelLeftClose,
  RefreshCw,
  Settings2,
  GitCompareArrows,
  UsersRound,
  UserRoundCog,
  Wifi,
  WifiOff,
  Zap,
  X,
} from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { NavLink, Outlet, useLocation } from "react-router-dom";
import { useAuth } from "../features/auth/AuthProvider";
import { friendlyApiError } from "../shared/api/client";
import { formatToday, roleLabel } from "../shared/lib/format";
import { useOnlineStatus } from "../shared/lib/useOnlineStatus";

interface NavigationItem {
  label: string;
  to?: string;
  icon: typeof LayoutDashboard;
  permission?: string;
  soon?: boolean;
}

const navigation: NavigationItem[] = [
  { label: "Tổng quan", to: "/dashboard", icon: LayoutDashboard },
  {
    label: "Ghi nhận công việc",
    to: "/operations/work",
    icon: Zap,
    permission: "business.urgent_work.read",
  },
  {
    label: "Ca kế hoạch của tôi",
    to: "/operations/my-shifts",
    icon: CalendarClock,
    permission: "business.staffing_work.self.read",
  },
  {
    label: "Điều phối ca",
    to: "/operations/shifts",
    icon: BriefcaseBusiness,
    permission: "business.shifts.read",
  },
  {
    label: "Đối soát",
    to: "/operations/reconciliation",
    icon: GitCompareArrows,
    permission: "business.reconciliation.read",
  },
  {
    label: "Khách hàng",
    to: "/operations/customers",
    icon: Building2,
    permission: "business.customers.read",
  },
  {
    label: "Giá và năng lực",
    to: "/operations/staffing-configuration",
    icon: Settings2,
    permission: "business.staffing_rates.read",
  },
  {
    label: "Nhân sự",
    icon: UsersRound,
    permission: "hr.employees.read",
    soon: true,
  },
  {
    label: "Tài khoản",
    to: "/admin/auth-users",
    icon: UserRoundCog,
    permission: "auth.accounts.read",
  },
];

function pageTitle(pathname: string): { title: string; description: string } {
  if (pathname === "/admin/auth-users") {
    return {
      title: "Quản trị tài khoản",
      description: "Cấp tài khoản và kiểm soát quyền đăng nhập cho doanh nghiệp.",
    };
  }

  if (pathname === "/operations/my-shifts") {
    return {
      title: "Ca kế hoạch của tôi",
      description: "Theo dõi các ca đã được điều phối viên tạo và phân công trước.",
    };
  }

  if (pathname === "/operations/work") {
    return {
      title: "Ghi nhận công việc",
      description: "Chọn cơ sở, bắt đầu hoặc kết thúc cho bạn và đồng nghiệp tại hiện trường.",
    };
  }

  if (pathname === "/operations/shifts") {
    return {
      title: "Điều phối ca",
      description: "Tạo yêu cầu khách hàng và phân công nhân viên phù hợp, không trùng lịch.",
    };
  }

  if (pathname === "/operations/staffing-configuration") {
    return {
      title: "Giá và năng lực dịch vụ",
      description: "Cấu hình độc lập giá thu khách hàng, tiền công nhân viên và năng lực làm dịch vụ.",
    };
  }

  if (pathname === "/operations/customers") {
    return {
      title: "Khách hàng",
      description: "Quản lý doanh nghiệp mua dịch vụ nhân sự và trạng thái hợp tác.",
    };
  }

  if (pathname === "/operations/reconciliation") {
    return {
      title: "Đối soát công việc phát sinh",
      description: "So sánh cả cơ sở và thời gian nhân viên ghi với bill của khách hàng.",
    };
  }

  if (pathname === "/operations/reconciliation/planned") {
    return {
      title: "Đối soát ca kế hoạch",
      description: "Đối soát các ca đã được tạo và phân công trước.",
    };
  }

  return {
    title: "Tổng quan vận hành",
    description: "Những thông tin cần chú ý trong hoạt động hôm nay.",
  };
}

export function OperationsLayout() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const location = useLocation();
  const isOnline = useOnlineStatus();
  const [mobileOpen, setMobileOpen] = useState(false);
  const [loggingOut, setLoggingOut] = useState(false);
  const [logoutError, setLogoutError] = useState<string | null>(null);
  const profile = auth.profile;
  const heading = pageTitle(location.pathname);

  if (!profile) {
    return null;
  }

  const visibleNavigation = navigation.filter(
    (item) => !item.permission || profile.permissions.includes(item.permission),
  );

  const logout = async () => {
    setLoggingOut(true);
    setLogoutError(null);
    try {
      await auth.logout();
      queryClient.clear();
    } catch (error) {
      setLogoutError(friendlyApiError(error, "Không thể đăng xuất lúc này. Vui lòng thử lại."));
    } finally {
      setLoggingOut(false);
    }
  };

  const sidebar = (
    <div className="flex h-full flex-col bg-slate-950 text-slate-300">
      <div className="flex h-20 items-center gap-3 border-b border-white/10 px-5">
        <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-blue-600 text-lg font-black text-white shadow-lg shadow-blue-600/20">
          S
        </div>
        <div className="min-w-0">
          <p className="truncate font-bold text-white">Shepherd</p>
          <p className="truncate text-xs text-slate-500">Điều hành nhân sự theo ca</p>
        </div>
        <button
          aria-label="Đóng trình đơn"
          className="ml-auto grid size-9 place-items-center rounded-lg text-slate-400 hover:bg-white/10 hover:text-white lg:hidden"
          onClick={() => setMobileOpen(false)}
          type="button"
        >
          <X className="size-5" />
        </button>
      </div>

      <nav className="flex-1 overflow-y-auto px-3 py-5">
        <p className="px-3 pb-2 text-[11px] font-bold uppercase tracking-[0.16em] text-slate-600">Vận hành</p>
        <div className="space-y-1">
          {visibleNavigation.map((item) => {
            const Icon = item.icon;
            if (!item.to) {
              return (
                <div
                  className="flex min-h-11 items-center gap-3 rounded-xl px-3 text-sm text-slate-600"
                  key={item.label}
                >
                  <Icon className="size-5" />
                  <span>{item.label}</span>
                  {item.soon ? (
                    <span className="ml-auto rounded-full bg-white/5 px-2 py-0.5 text-[10px] font-semibold">Sắp có</span>
                  ) : null}
                </div>
              );
            }

            return (
              <NavLink
                className={({ isActive }) =>
                  `flex min-h-11 items-center gap-3 rounded-xl px-3 text-sm font-semibold transition ${
                    isActive
                      ? "bg-blue-600 text-white shadow-lg shadow-blue-950/30"
                      : "text-slate-400 hover:bg-white/5 hover:text-white"
                  }`
                }
                key={item.label}
                onClick={() => setMobileOpen(false)}
                to={item.to}
              >
                <Icon className="size-5" />
                <span>{item.label}</span>
                <ChevronRight className="ml-auto size-4 opacity-50" />
              </NavLink>
            );
          })}
        </div>
      </nav>

      <div className="border-t border-white/10 p-4">
        <div className="mb-3 flex items-center gap-3 rounded-xl bg-white/5 p-3">
          <div className="grid size-9 shrink-0 place-items-center rounded-full bg-slate-800 text-sm font-bold text-blue-300">
            {profile.username.slice(0, 1).toUpperCase()}
          </div>
          <div className="min-w-0">
            <p className="truncate text-sm font-semibold text-white">{profile.username}</p>
            <p className="truncate text-xs text-slate-500">{roleLabel(profile.primary_role)}</p>
          </div>
        </div>
        <button
          className="flex min-h-10 w-full items-center justify-center gap-2 rounded-xl border border-white/10 text-sm font-semibold text-slate-300 transition hover:bg-white/5 hover:text-white disabled:opacity-60"
          disabled={loggingOut}
          onClick={() => void logout()}
          type="button"
        >
          {loggingOut ? <RefreshCw className="size-4 animate-spin" /> : <LogOut className="size-4" />}
          {loggingOut ? "Đang đăng xuất..." : "Đăng xuất"}
        </button>
        {logoutError ? <p className="mt-2 text-xs leading-5 text-red-300">{logoutError}</p> : null}
      </div>
    </div>
  );

  return (
    <div className="min-h-screen bg-slate-50 lg:grid lg:grid-cols-[272px_minmax(0,1fr)]">
      <aside className="sticky top-0 hidden h-screen lg:block">{sidebar}</aside>

      {mobileOpen ? (
        <div className="fixed inset-0 z-50 lg:hidden">
          <button
            aria-label="Đóng lớp phủ"
            className="absolute inset-0 bg-slate-950/55 backdrop-blur-sm"
            onClick={() => setMobileOpen(false)}
            type="button"
          />
          <aside className="relative h-full w-[min(86vw,300px)] shadow-2xl">{sidebar}</aside>
        </div>
      ) : null}

      <div className="min-w-0">
        <header className="sticky top-0 z-30 border-b border-slate-200/80 bg-white/90 backdrop-blur-xl">
          <div className="flex h-20 items-center gap-4 px-4 sm:px-6 lg:px-8">
            <button
              aria-label="Mở trình đơn"
              className="grid size-10 shrink-0 place-items-center rounded-xl border border-slate-200 text-slate-600 hover:bg-slate-50 lg:hidden"
              onClick={() => setMobileOpen(true)}
              type="button"
            >
              <Menu className="size-5" />
            </button>

            <div className="min-w-0">
              <h1 className="truncate text-lg font-bold text-slate-950 sm:text-xl">{heading.title}</h1>
              <p className="hidden truncate text-sm text-slate-500 sm:block">{heading.description}</p>
            </div>

            <div className="ml-auto flex items-center gap-3">
              <div
                className={`hidden items-center gap-2 rounded-full px-3 py-1.5 text-xs font-semibold sm:flex ${
                  isOnline ? "bg-emerald-50 text-emerald-700" : "bg-amber-50 text-amber-700"
                }`}
              >
                {isOnline ? <Wifi className="size-3.5" /> : <WifiOff className="size-3.5" />}
                {isOnline ? "Đang kết nối" : "Ngoại tuyến"}
              </div>
              <div className="hidden h-8 w-px bg-slate-200 sm:block" />
              <div className="hidden text-right md:block">
                <p className="text-sm font-semibold text-slate-800">{profile.username}</p>
                <p className="text-xs text-slate-500">{roleLabel(profile.primary_role)}</p>
              </div>
              <CircleUserRound className="size-9 text-slate-400" />
            </div>
          </div>
        </header>

        {!isOnline ? (
          <div className="border-b border-amber-200 bg-amber-50 px-4 py-2 text-center text-sm font-medium text-amber-800">
            Bạn đang ngoại tuyến. Dữ liệu hiện tại có thể đã cũ và thao tác mới sẽ bị tạm dừng.
          </div>
        ) : null}

        <main className="mx-auto w-full max-w-[1480px] p-4 sm:p-6 lg:p-8">
          <div className="mb-6 flex items-center gap-2 text-sm capitalize text-slate-500">
            <PanelLeftClose className="size-4 text-blue-600" />
            {formatToday()}
          </div>
          <Outlet />
        </main>
      </div>
    </div>
  );
}
