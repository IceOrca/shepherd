import type { Branch, BranchCreateRequest, BranchPageResponse, BranchUpdateRequest } from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export const branchQueryKeys = {
  all: ["branches", "management"] as const,
};

export function listManagedBranches(cursor: string | null = null): Promise<BranchPageResponse> {
  const query: string = cursor === null ? "" : `?cursor=${encodeURIComponent(cursor)}`;
  return apiRequest<BranchPageResponse>(`/api/business/branches/manage${query}`);
}

export function createBranch(request: BranchCreateRequest): Promise<Branch> {
  return apiRequest<Branch>("/api/business/branches", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export function updateBranch(branchId: string, request: BranchUpdateRequest): Promise<Branch> {
  return apiRequest<Branch>(`/api/business/branches/${encodeURIComponent(branchId)}`, {
    method: "PUT",
    body: JSON.stringify(request),
  });
}
