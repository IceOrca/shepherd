export const PLANNED_STAFFING_ENABLED: boolean =
  import.meta.env.VITE_PLANNED_STAFFING_ENABLED === "true";

export function isPlannedStaffingPermission(permissionCode: string): boolean {
  return permissionCode.startsWith("business.shifts.")
    || permissionCode.startsWith("business.staffing_work.")
    || permissionCode === "business.reconciliation.manage";
}

export function assertPlannedStaffingEnabled(): void {
  if (!PLANNED_STAFFING_ENABLED) {
    throw new Error("Planned staffing is disabled in this build.");
  }
}
