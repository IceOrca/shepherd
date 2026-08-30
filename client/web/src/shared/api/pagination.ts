export interface CursorApiPage<T> {
  items: T[];
  next_cursor: string | null;
  has_more: boolean;
  limit: number;
}

export async function collectCursorPages<T>(
  fetchPage: (cursor: string | null) => Promise<CursorApiPage<T>>,
): Promise<T[]> {
  const items: T[] = [];
  const seenCursors: Set<string> = new Set<string>();
  let cursor: string | null = null;
  for (;;) {
    const page: CursorApiPage<T> = await fetchPage(cursor);
    if (page.has_more !== (page.next_cursor !== null)) {
      throw new Error("cursor metadata returned by the server is inconsistent");
    }
    items.push(...page.items);
    if (page.next_cursor === null) {
      return items;
    }
    if (seenCursors.has(page.next_cursor)) {
      throw new Error("cursor returned by the server did not advance");
    }
    seenCursors.add(page.next_cursor);
    cursor = page.next_cursor;
  }
}
