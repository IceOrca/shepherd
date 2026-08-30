export interface ReconciliationApiPage<T> {
  items: T[];
  next_cursor: string | null;
  has_more: boolean;
  limit: number;
}

interface BranchCursorState<T> {
  buffer: T[];
  cursor: string | null;
  exhausted: boolean;
}

export interface ReconciliationScopeCursor<T> {
  branches: Record<string, BranchCursorState<T>>;
  limit: number | null;
}

export interface ReconciliationScopePage<T> {
  items: T[];
  nextCursor: ReconciliationScopeCursor<T> | null;
  limit: number;
}

export function createReconciliationScopeCursor<T>(
  branchIds: string[],
): ReconciliationScopeCursor<T> {
  return {
    branches: Object.fromEntries(
      branchIds.map((branchId: string): [string, BranchCursorState<T>] => [
        branchId,
        { buffer: [], cursor: null, exhausted: false },
      ]),
    ),
    limit: null,
  };
}

export async function loadReconciliationScopePage<T>({
  cursor,
  fetchBranchPage,
  compare,
  itemKey,
}: {
  cursor: ReconciliationScopeCursor<T>;
  fetchBranchPage: (
    branchId: string,
    cursor: string | null,
  ) => Promise<ReconciliationApiPage<T>>;
  compare: (left: T, right: T) => number;
  itemKey: (item: T) => string;
}): Promise<ReconciliationScopePage<T>> {
  const branches: Record<string, BranchCursorState<T>> = Object.fromEntries(
    Object.entries(cursor.branches).map(
      ([branchId, state]: [string, BranchCursorState<T>]): [string, BranchCursorState<T>] => [
        branchId,
        { ...state, buffer: [...state.buffer] },
      ],
    ),
  );
  let resolvedLimit: number | null = cursor.limit;
  const branchIdsToRefill: string[] = Object.entries(branches)
    .filter(([, state]: [string, BranchCursorState<T>]): boolean =>
      state.buffer.length === 0 && !state.exhausted,
    )
    .map(([branchId]: [string, BranchCursorState<T>]): string => branchId);

  await Promise.all(
    branchIdsToRefill.map(async (branchId: string): Promise<void> => {
      const state: BranchCursorState<T> = branches[branchId];
      const page: ReconciliationApiPage<T> = await fetchBranchPage(branchId, state.cursor);
      if (page.has_more !== (page.next_cursor !== null)) {
        throw new Error("reconciliation cursor metadata returned by the server is inconsistent");
      }
      if (resolvedLimit !== null && resolvedLimit !== page.limit) {
        throw new Error("reconciliation page-size configuration differs between branch requests");
      }
      resolvedLimit = page.limit;
      branches[branchId] = {
        buffer: page.items,
        cursor: page.next_cursor,
        exhausted: !page.has_more,
      };
    }),
  );

  if (resolvedLimit === null) {
    throw new Error("reconciliation page size was not returned by the server");
  }

  const candidates: T[] = Object.values(branches)
    .flatMap((state: BranchCursorState<T>): T[] => state.buffer)
    .sort(compare);
  const items: T[] = candidates.slice(0, resolvedLimit);
  const selectedKeys: Set<string> = new Set<string>(items.map(itemKey));
  const branchStates: BranchCursorState<T>[] = Object.values(branches);
  for (const state of branchStates) {
    state.buffer = state.buffer.filter((item: T): boolean => !selectedKeys.has(itemKey(item)));
  }

  const hasMore: boolean = branchStates.some(
    (state: BranchCursorState<T>): boolean => state.buffer.length > 0 || !state.exhausted,
  );
  return {
    items,
    nextCursor: hasMore ? { branches, limit: resolvedLimit } : null,
    limit: resolvedLimit,
  };
}
