import {
  useEffect,
  useMemo,
  useState,
  type ChangeEvent,
  type Dispatch,
  type SetStateAction,
} from "react";

export interface DateRange {
  start: string;
  end: string;
}

export interface MonthRangeFilterController {
  startDate: string;
  endDate: string;
  setStartDate: Dispatch<SetStateAction<string>>;
  setEndDate: Dispatch<SetStateAction<string>>;
}

export function localDateInput(date: Date): string {
  const offset: number = date.getTimezoneOffset();
  return new Date(date.getTime() - offset * 60_000).toISOString().slice(0, 10);
}

export function currentMonthRange(): DateRange {
  const now: Date = new Date();
  return {
    start: localDateInput(new Date(now.getFullYear(), now.getMonth(), 1)),
    end: localDateInput(new Date(now.getFullYear(), now.getMonth() + 1, 0)),
  };
}

export function monthRange(month: string): DateRange | null {
  if (!/^\d{4}-\d{2}$/.test(month)) return null;
  const [yearText, monthText]: string[] = month.split("-");
  const year: number = Number(yearText);
  const monthIndex: number = Number(monthText) - 1;
  if (!Number.isInteger(year) || monthIndex < 0 || monthIndex > 11) return null;
  return {
    start: localDateInput(new Date(year, monthIndex, 1)),
    end: localDateInput(new Date(year, monthIndex + 1, 0)),
  };
}

export function selectedMonthForRange(start: string, end: string): string {
  const month: string = start.slice(0, 7);
  const range: DateRange | null = monthRange(month);
  return range?.start === start && range.end === end ? month : "";
}

export function useMonthRangeFilter(): MonthRangeFilterController {
  const initialRange: DateRange = useMemo(currentMonthRange, []);
  const [startDate, setStartDate] = useState<string>(initialRange.start);
  const [endDate, setEndDate] = useState<string>(initialRange.end);
  return { startDate, endDate, setStartDate, setEndDate };
}

export function MonthRangeFilterFields({
  controller,
}: {
  controller: MonthRangeFilterController;
}): React.JSX.Element {
  const { startDate, endDate, setStartDate, setEndDate }: MonthRangeFilterController = controller;
  const [selectedMonth, setSelectedMonth] = useState<string>(() =>
    selectedMonthForRange(startDate, endDate),
  );

  useEffect((): void => {
    setSelectedMonth(selectedMonthForRange(startDate, endDate));
  }, [endDate, startDate]);

  const changeMonth = (event: ChangeEvent<HTMLInputElement>): void => {
    const month: string = event.target.value;
    setSelectedMonth(month);
    const range: DateRange | null = monthRange(month);
    if (range) {
      setStartDate(range.start);
      setEndDate(range.end);
    }
  };

  const changeStartDate = (event: ChangeEvent<HTMLInputElement>): void => {
    const value: string = event.target.value;
    setStartDate(value);
    setSelectedMonth(selectedMonthForRange(value, endDate));
  };

  const changeEndDate = (event: ChangeEvent<HTMLInputElement>): void => {
    const value: string = event.target.value;
    setEndDate(value);
    setSelectedMonth(selectedMonthForRange(startDate, value));
  };

  return (
    <>
      <label className="text-sm font-semibold text-slate-700">
        Chọn nhanh theo tháng
        <input
          className="mt-2 min-h-11 w-full rounded-xl border-slate-300"
          onChange={changeMonth}
          type="month"
          value={selectedMonth}
        />
      </label>
      <label className="text-sm font-semibold text-slate-700">
        Từ ngày
        <input
          className="mt-2 min-h-11 w-full rounded-xl border-slate-300"
          onChange={changeStartDate}
          type="date"
          value={startDate}
        />
      </label>
      <label className="text-sm font-semibold text-slate-700">
        Đến ngày
        <input
          className="mt-2 min-h-11 w-full rounded-xl border-slate-300"
          min={startDate}
          onChange={changeEndDate}
          type="date"
          value={endDate}
        />
      </label>
    </>
  );
}
