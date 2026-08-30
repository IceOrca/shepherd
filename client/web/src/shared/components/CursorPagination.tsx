import { ChevronLeft, ChevronRight, LoaderCircle } from "lucide-react";

export function CursorPagination({
  currentPage,
  currentItemCount,
  hasNextPage,
  nextPagePending,
  onPageChange,
  className = "",
}: {
  currentPage: number;
  currentItemCount: number;
  hasNextPage: boolean;
  nextPagePending: boolean;
  onPageChange: (page: number) => void;
  className?: string;
}): React.JSX.Element | null {
  if (currentItemCount === 0) {
    return null;
  }

  return (
    <nav aria-label="Phân trang kết quả" className={`flex items-center justify-between gap-3 border-t border-slate-200 px-4 py-3 ${className}`}>
      <button aria-label="Trang trước" className="grid size-11 shrink-0 place-items-center rounded-xl border border-slate-200 text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-40" disabled={currentPage <= 1} onClick={(): void => onPageChange(currentPage - 1)} type="button"><ChevronLeft className="size-5" /></button>
      <div className="min-w-0 text-center"><p className="text-sm font-bold text-slate-800">Trang {currentPage}</p><p className="mt-0.5 text-xs text-slate-500">{currentItemCount} kết quả trên trang</p></div>
      <button aria-label="Trang sau" className="grid size-11 shrink-0 place-items-center rounded-xl border border-slate-200 text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-40" disabled={!hasNextPage || nextPagePending} onClick={(): void => onPageChange(currentPage + 1)} type="button">{nextPagePending ? <LoaderCircle className="size-5 animate-spin" /> : <ChevronRight className="size-5" />}</button>
    </nav>
  );
}
