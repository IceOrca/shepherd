import type { RoleCode, ShiftAssignmentStatus, StaffingShiftStatus } from "../../api/generated/contracts";

const dateTimeFormatter = new Intl.DateTimeFormat("vi-VN", {
  dateStyle: "medium",
  timeStyle: "short",
});

const dateFormatter = new Intl.DateTimeFormat("vi-VN", {
  weekday: "long",
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
});

export function formatDateTime(value: string): string {
  return dateTimeFormatter.format(new Date(value));
}

export function formatToday(): string {
  return dateFormatter.format(new Date());
}

export function formatDuration(totalSeconds: number): string {
  const safeSeconds = Math.max(0, totalSeconds);
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);

  if (hours === 0) {
    return `${minutes} phút`;
  }

  return `${hours} giờ ${minutes.toString().padStart(2, "0")} phút`;
}

export function roleLabel(role: RoleCode): string {
  switch (role) {
    case "owner":
      return "Chủ doanh nghiệp";
    case "director":
      return "Giám đốc";
    case "manager":
      return "Quản lý";
    case "supervisor":
      return "Giám sát";
    case "staff":
      return "Nhân viên";
    default:
      return role;
  }
}

export function assignmentStatusLabel(status: ShiftAssignmentStatus): string {
  switch (status) {
    case "assigned":
      return "Đã phân công";
    case "approved":
      return "Đã duyệt";
    case "cancelled":
      return "Đã hủy";
  }
}

export function shiftStatusLabel(status: StaffingShiftStatus): string {
  switch (status) {
    case "open":
      return "Đang cần người";
    case "filled":
      return "Đã đủ người";
    case "in_progress":
      return "Đang diễn ra";
    case "completed":
      return "Đã hoàn thành";
    case "cancelled":
      return "Đã hủy";
  }
}
