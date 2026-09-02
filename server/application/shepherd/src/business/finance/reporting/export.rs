use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook, Worksheet, XlsxError};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::auth::RoleCode;

use super::core::{
    FinancialPeriodState, FinancialPeriodStatus, OperatingFinancialLine, OperatingFinancialReport, PayrollLine,
    PayrollReport,
};

const MONEY_SCALE: i128 = 10_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ReportExportKind {
    OperatingFinancial,
    Payroll,
}

impl ReportExportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OperatingFinancial => "operating_financial",
            Self::Payroll => "payroll",
        }
    }

    pub fn filename_prefix(self) -> &'static str {
        match self {
            Self::OperatingFinancial => "bao-cao-tai-chinh",
            Self::Payroll => "bang-luong",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReportExportMetadata {
    pub tenant_name: String,
    pub actor_username: String,
    pub generated_at: DateTime<Utc>,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

#[derive(Clone, Debug)]
pub struct FinancialPeriodExportState {
    pub branch_name: String,
    pub state: FinancialPeriodState,
}

#[derive(Debug)]
pub struct GeneratedWorkbook {
    pub bytes: Vec<u8>,
    pub row_count: usize,
    pub currencies: Vec<String>,
    pub contains_open_period: bool,
    pub warning_count: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct FinancialAmounts {
    staffing_revenue: i128,
    staffing_worker_cost: i128,
    coordination_salary_cost: i128,
    approved_business_expense: i128,
    profit_share_cost: i128,
    operating_cost: i128,
    operating_profit: i128,
    business_profit_after_profit_share: i128,
    reimbursed_cash: i128,
    salary_advance_disbursed: i128,
    salary_advance_recovered: i128,
    outstanding_expense_reimbursement: i128,
    outstanding_salary_advance: i128,
}

impl FinancialAmounts {
    fn add_line(&mut self, line: &OperatingFinancialLine) -> Result<(), XlsxError> {
        self.staffing_revenue = add_decimal(self.staffing_revenue, &line.staffing_revenue)?;
        self.staffing_worker_cost = add_decimal(self.staffing_worker_cost, &line.staffing_worker_cost)?;
        self.coordination_salary_cost = add_decimal(self.coordination_salary_cost, &line.coordination_salary_cost)?;
        self.approved_business_expense = add_decimal(self.approved_business_expense, &line.approved_business_expense)?;
        self.profit_share_cost = add_decimal(self.profit_share_cost, &line.profit_share_cost)?;
        self.operating_cost = add_decimal(self.operating_cost, &line.operating_cost)?;
        self.operating_profit = add_decimal(self.operating_profit, &line.operating_profit)?;
        self.business_profit_after_profit_share = add_decimal(
            self.business_profit_after_profit_share,
            &line.business_profit_after_profit_share,
        )?;
        self.reimbursed_cash = add_decimal(self.reimbursed_cash, &line.reimbursed_cash)?;
        self.salary_advance_disbursed = add_decimal(self.salary_advance_disbursed, &line.salary_advance_disbursed)?;
        self.salary_advance_recovered = add_decimal(self.salary_advance_recovered, &line.salary_advance_recovered)?;
        self.outstanding_expense_reimbursement = add_decimal(
            self.outstanding_expense_reimbursement,
            &line.outstanding_expense_reimbursement,
        )?;
        self.outstanding_salary_advance =
            add_decimal(self.outstanding_salary_advance, &line.outstanding_salary_advance)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PayrollAmounts {
    staffing_earnings: i128,
    prorated_monthly_salary: i128,
    profit_share_payment: i128,
    gross_pay: i128,
    recorded_expense_reimbursement: i128,
    suggested_expense_reimbursement: i128,
    recorded_advance_deduction: i128,
    suggested_advance_deduction: i128,
    estimated_net_pay: i128,
}

impl PayrollAmounts {
    fn add_line(&mut self, line: &PayrollLine) -> Result<(), XlsxError> {
        self.staffing_earnings = add_decimal(self.staffing_earnings, &line.staffing_earnings)?;
        self.prorated_monthly_salary = add_decimal(self.prorated_monthly_salary, &line.prorated_monthly_salary)?;
        self.profit_share_payment = add_decimal(self.profit_share_payment, &line.profit_share_payment)?;
        self.gross_pay = add_decimal(self.gross_pay, &line.gross_pay)?;
        self.recorded_expense_reimbursement = add_decimal(
            self.recorded_expense_reimbursement,
            &line.recorded_expense_reimbursement,
        )?;
        self.suggested_expense_reimbursement = add_decimal(
            self.suggested_expense_reimbursement,
            &line.suggested_expense_reimbursement,
        )?;
        self.recorded_advance_deduction =
            add_decimal(self.recorded_advance_deduction, &line.recorded_advance_deduction)?;
        self.suggested_advance_deduction =
            add_decimal(self.suggested_advance_deduction, &line.suggested_advance_deduction)?;
        self.estimated_net_pay = add_decimal(self.estimated_net_pay, &line.estimated_net_pay)?;
        Ok(())
    }
}

pub fn build_financial_workbook(
    metadata: &ReportExportMetadata,
    reports: &[OperatingFinancialReport],
    periods: &[FinancialPeriodExportState],
) -> Result<GeneratedWorkbook, XlsxError> {
    let row_count: usize = reports
        .iter()
        .fold(0usize, |total, report| total.saturating_add(report.lines.len()));
    let currencies: Vec<String> = financial_currencies(reports);
    let open_period_count: usize = open_period_count(periods);
    let mut workbook: Workbook = Workbook::new();
    write_information_sheet(
        &mut workbook,
        metadata,
        ReportExportKind::OperatingFinancial,
        reports.len(),
        periods,
    )?;
    write_financial_summary_sheet(&mut workbook, reports)?;
    write_financial_branch_sheet(&mut workbook, reports)?;
    let bytes: Vec<u8> = workbook.save_to_buffer()?;
    Ok(GeneratedWorkbook {
        bytes,
        row_count,
        currencies,
        contains_open_period: open_period_count > 0,
        warning_count: open_period_count,
    })
}

pub fn build_payroll_workbook(
    metadata: &ReportExportMetadata,
    reports: &[PayrollReport],
    periods: &[FinancialPeriodExportState],
) -> Result<GeneratedWorkbook, XlsxError> {
    let row_count: usize = reports
        .iter()
        .fold(0usize, |total, report| total.saturating_add(report.lines.len()));
    let currencies: Vec<String> = payroll_currencies(reports);
    let overlap_count: usize = reports
        .iter()
        .flat_map(|report| report.lines.iter())
        .fold(0usize, |total, line| {
            total.saturating_add(usize::try_from(line.attendance_overlap_count.max(0)).unwrap_or(usize::MAX))
        });
    let open_period_count: usize = open_period_count(periods);
    let mut workbook: Workbook = Workbook::new();
    write_information_sheet(
        &mut workbook,
        metadata,
        ReportExportKind::Payroll,
        reports.len(),
        periods,
    )?;
    write_payroll_summary_sheet(&mut workbook, reports)?;
    write_payroll_detail_sheet(&mut workbook, reports)?;
    write_payroll_warning_sheet(&mut workbook, reports)?;
    let bytes: Vec<u8> = workbook.save_to_buffer()?;
    Ok(GeneratedWorkbook {
        bytes,
        row_count,
        currencies,
        contains_open_period: open_period_count > 0,
        warning_count: open_period_count.saturating_add(overlap_count),
    })
}

fn financial_currencies(reports: &[OperatingFinancialReport]) -> Vec<String> {
    reports
        .iter()
        .flat_map(|report| report.lines.iter().map(|line| line.currency.clone()))
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect()
}

fn payroll_currencies(reports: &[PayrollReport]) -> Vec<String> {
    reports
        .iter()
        .flat_map(|report| report.lines.iter().map(|line| line.currency.clone()))
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect()
}

fn open_period_count(periods: &[FinancialPeriodExportState]) -> usize {
    periods
        .iter()
        .filter(|period| period.state.status == FinancialPeriodStatus::Open)
        .count()
}

fn write_information_sheet(
    workbook: &mut Workbook,
    metadata: &ReportExportMetadata,
    kind: ReportExportKind,
    branch_count: usize,
    periods: &[FinancialPeriodExportState],
) -> Result<(), XlsxError> {
    let title: Format = title_format();
    let label: Format = label_format();
    let date: Format = date_only_format();
    let datetime: Format = datetime_format();
    let warning: Format = warning_format();
    let worksheet: &mut Worksheet = workbook.add_worksheet();
    worksheet.set_name("Thông tin")?;
    worksheet.set_column_width(0, 25.0)?;
    worksheet.set_column_width(1, 48.0)?;
    worksheet.write_string_with_format(0, 0, report_title(kind), &title)?;
    write_label_value(worksheet, 2, "Doanh nghiệp", &metadata.tenant_name, &label)?;
    write_label_value(worksheet, 3, "Người xuất", &metadata.actor_username, &label)?;
    worksheet.write_string_with_format(4, 0, "Tạo lúc (UTC)", &label)?;
    worksheet.write_datetime_with_format(4, 1, metadata.generated_at.naive_utc(), &datetime)?;
    worksheet.write_string_with_format(5, 0, "Từ ngày", &label)?;
    worksheet.write_datetime_with_format(5, 1, metadata.start_date, &date)?;
    worksheet.write_string_with_format(6, 0, "Đến ngày", &label)?;
    worksheet.write_datetime_with_format(6, 1, metadata.end_date, &date)?;
    write_label_value(worksheet, 7, "Số chi nhánh", &branch_count.to_string(), &label)?;
    worksheet.write_string_with_format(9, 0, "Trạng thái kỳ tài chính", &title)?;
    write_headers_at(worksheet, 10, &["Chi nhánh", "Kỳ", "Trạng thái"])?;
    worksheet.set_column_width(2, 18.0)?;
    for (offset, period) in periods.iter().enumerate() {
        let row: u32 = checked_row(offset, 11)?;
        worksheet.write_string(row, 0, &period.branch_name)?;
        worksheet.write_datetime_with_format(row, 1, period.state.period_start, &date)?;
        let status: &str = match period.state.status {
            FinancialPeriodStatus::Open => "Đang mở",
            FinancialPeriodStatus::Closed => "Đã khóa",
        };
        if period.state.status == FinancialPeriodStatus::Open {
            worksheet.write_string_with_format(row, 2, status, &warning)?;
        } else {
            worksheet.write_string(row, 2, status)?;
        }
    }
    if !periods.is_empty() {
        worksheet.autofilter(10, 0, checked_row(periods.len() - 1, 11)?, 2)?;
        worksheet.set_freeze_panes(11, 0)?;
    }
    Ok(())
}

fn write_financial_summary_sheet(
    workbook: &mut Workbook,
    reports: &[OperatingFinancialReport],
) -> Result<(), XlsxError> {
    let mut totals: BTreeMap<String, FinancialAmounts> = BTreeMap::new();
    for line in reports.iter().flat_map(|report| &report.lines) {
        totals.entry(line.currency.clone()).or_default().add_line(line)?;
    }
    let worksheet: &mut Worksheet = workbook.add_worksheet();
    worksheet.set_name("Tổng hợp")?;
    worksheet.set_freeze_panes(1, 1)?;
    write_headers(worksheet, &financial_headings(false))?;
    for (offset, (currency, amount)) in totals.iter().enumerate() {
        let row: u32 = checked_row(offset, 1)?;
        worksheet.write_string(row, 0, currency)?;
        write_financial_amounts(worksheet, row, amount)?;
    }
    configure_money_columns(worksheet, 0, 13)?;
    if !totals.is_empty() {
        worksheet.autofilter(0, 0, checked_row(totals.len() - 1, 1)?, 13)?;
    }
    Ok(())
}

fn write_financial_branch_sheet(
    workbook: &mut Workbook,
    reports: &[OperatingFinancialReport],
) -> Result<(), XlsxError> {
    let worksheet: &mut Worksheet = workbook.add_worksheet();
    worksheet.set_name("Theo chi nhánh")?;
    worksheet.set_freeze_panes(1, 2)?;
    write_headers(worksheet, &financial_headings(true))?;
    let mut row: u32 = 1;
    for report in reports {
        for line in &report.lines {
            worksheet.write_string(row, 0, &report.branch_name)?;
            worksheet.write_string(row, 1, &line.currency)?;
            write_financial_line(worksheet, row, line, 2)?;
            row = next_row(row)?;
        }
    }
    worksheet.set_column_width(0, 28.0)?;
    configure_money_columns(worksheet, 1, 14)?;
    if row > 1 {
        worksheet.autofilter(0, 0, row - 1, 14)?;
    }
    Ok(())
}

fn write_payroll_summary_sheet(workbook: &mut Workbook, reports: &[PayrollReport]) -> Result<(), XlsxError> {
    let mut totals: BTreeMap<String, PayrollAmounts> = BTreeMap::new();
    for line in reports.iter().flat_map(|report| &report.lines) {
        totals.entry(line.currency.clone()).or_default().add_line(line)?;
    }
    let worksheet: &mut Worksheet = workbook.add_worksheet();
    worksheet.set_name("Tổng hợp")?;
    worksheet.set_freeze_panes(1, 1)?;
    write_headers(
        worksheet,
        &[
            "Tiền tệ",
            "Tiền công",
            "Lương tháng phân bổ",
            "Thưởng theo lợi nhuận",
            "Lương gộp",
            "Hoàn chi hộ đã tính",
            "Hoàn chi hộ khi khóa",
            "Tạm ứng đã khấu trừ",
            "Tạm ứng khấu trừ khi khóa",
            "Thực trả",
        ],
    )?;
    for (offset, (currency, amount)) in totals.iter().enumerate() {
        let row: u32 = checked_row(offset, 1)?;
        worksheet.write_string(row, 0, currency)?;
        write_money_scaled(worksheet, row, 1, amount.staffing_earnings)?;
        write_money_scaled(worksheet, row, 2, amount.prorated_monthly_salary)?;
        write_money_scaled(worksheet, row, 3, amount.profit_share_payment)?;
        write_money_scaled(worksheet, row, 4, amount.gross_pay)?;
        write_money_scaled(worksheet, row, 5, amount.recorded_expense_reimbursement)?;
        write_money_scaled(worksheet, row, 6, amount.suggested_expense_reimbursement)?;
        write_money_scaled(worksheet, row, 7, amount.recorded_advance_deduction)?;
        write_money_scaled(worksheet, row, 8, amount.suggested_advance_deduction)?;
        write_money_scaled(worksheet, row, 9, amount.estimated_net_pay)?;
    }
    configure_money_columns(worksheet, 0, 9)?;
    if !totals.is_empty() {
        worksheet.autofilter(0, 0, checked_row(totals.len() - 1, 1)?, 9)?;
    }
    Ok(())
}

fn write_payroll_detail_sheet(workbook: &mut Workbook, reports: &[PayrollReport]) -> Result<(), XlsxError> {
    let worksheet: &mut Worksheet = workbook.add_worksheet();
    worksheet.set_name("Bảng lương")?;
    worksheet.set_freeze_panes(1, 4)?;
    write_headers(
        worksheet,
        &[
            "Chi nhánh",
            "Mã nhân viên",
            "Nhân viên",
            "Vai trò",
            "Tiền tệ",
            "Giờ làm Staff",
            "Tiền công",
            "Lương tháng phân bổ",
            "Lợi nhuận làm căn cứ",
            "Tỷ lệ thưởng (%)",
            "Thưởng theo lợi nhuận",
            "Trạng thái thưởng",
            "Lương gộp",
            "Hoàn chi hộ đã tính",
            "Hoàn chi hộ khi khóa",
            "Tạm ứng đã khấu trừ",
            "Tạm ứng khấu trừ khi khóa",
            "Thực trả",
            "Cảnh báo trùng nguồn",
        ],
    )?;
    let money: Format = money_format();
    let duration: Format = duration_format();
    let warning: Format = warning_format();
    let mut row: u32 = 1;
    for report in reports {
        for line in &report.lines {
            worksheet.write_string(row, 0, &report.branch_name)?;
            worksheet.write_string(row, 1, &line.employee_code)?;
            worksheet.write_string(row, 2, &line.employee_name)?;
            worksheet.write_string(row, 3, role_label(&line.role))?;
            worksheet.write_string(row, 4, &line.currency)?;
            worksheet.write_number_with_format(row, 5, duration_number(line.staffing_worked_seconds)?, &duration)?;
            for (column, value) in [
                (6, &line.staffing_earnings),
                (7, &line.prorated_monthly_salary),
                (8, &line.profit_share_base),
                (10, &line.profit_share_payment),
                (12, &line.gross_pay),
                (13, &line.recorded_expense_reimbursement),
                (14, &line.suggested_expense_reimbursement),
                (15, &line.recorded_advance_deduction),
                (16, &line.suggested_advance_deduction),
                (17, &line.estimated_net_pay),
            ] {
                worksheet.write_number_with_format(row, column, excel_number(value)?, &money)?;
            }
            worksheet.write_number(row, 9, excel_number(&line.profit_share_percent)?)?;
            worksheet.write_string(
                row,
                11,
                if line.profit_share_locked {
                    "Đã khóa"
                } else {
                    "Tạm tính"
                },
            )?;
            if line.attendance_overlap_count > 0 {
                worksheet.write_number_with_format(
                    row,
                    18,
                    integer_number(line.attendance_overlap_count)?,
                    &warning,
                )?;
            } else {
                worksheet.write_number(row, 18, 0.0)?;
            }
            row = next_row(row)?;
        }
    }
    worksheet.set_column_width(0, 27.0)?;
    worksheet.set_column_width(1, 16.0)?;
    worksheet.set_column_width(2, 27.0)?;
    worksheet.set_column_width(3, 18.0)?;
    worksheet.set_column_width(4, 11.0)?;
    worksheet.set_column_width(5, 16.0)?;
    for column in 6..=17 {
        worksheet.set_column_width(column, 21.0)?;
    }
    worksheet.set_column_width(18, 23.0)?;
    if row > 1 {
        worksheet.autofilter(0, 0, row - 1, 18)?;
    }
    Ok(())
}

fn write_payroll_warning_sheet(workbook: &mut Workbook, reports: &[PayrollReport]) -> Result<(), XlsxError> {
    let worksheet: &mut Worksheet = workbook.add_worksheet();
    worksheet.set_name("Cảnh báo")?;
    write_headers(
        worksheet,
        &[
            "Chi nhánh",
            "Mã nhân viên",
            "Nhân viên",
            "Tiền tệ",
            "Số khoảng trùng nguồn",
        ],
    )?;
    let warning: Format = warning_format();
    let mut row: u32 = 1;
    for report in reports {
        for line in &report.lines {
            if line.attendance_overlap_count <= 0 {
                continue;
            }
            worksheet.write_string(row, 0, &report.branch_name)?;
            worksheet.write_string(row, 1, &line.employee_code)?;
            worksheet.write_string(row, 2, &line.employee_name)?;
            worksheet.write_string(row, 3, &line.currency)?;
            worksheet.write_number_with_format(row, 4, integer_number(line.attendance_overlap_count)?, &warning)?;
            row = next_row(row)?;
        }
    }
    if row == 1 {
        worksheet.write_string(
            1,
            0,
            "Không có khoảng làm việc trùng giữa công việc khách hàng và chấm công nội bộ.",
        )?;
    } else {
        worksheet.autofilter(0, 0, row - 1, 4)?;
    }
    worksheet.set_freeze_panes(1, 0)?;
    worksheet.set_column_width(0, 27.0)?;
    worksheet.set_column_width(1, 16.0)?;
    worksheet.set_column_width(2, 27.0)?;
    worksheet.set_column_width(3, 11.0)?;
    worksheet.set_column_width(4, 24.0)?;
    Ok(())
}

fn financial_headings(with_branch: bool) -> Vec<&'static str> {
    let mut headings: Vec<&str> = Vec::with_capacity(if with_branch { 15 } else { 14 });
    if with_branch {
        headings.push("Chi nhánh");
    }
    headings.extend([
        "Tiền tệ",
        "Doanh thu",
        "Tiền công Staff",
        "Lương quản lý",
        "Chi phí khác",
        "Thưởng theo lợi nhuận (tách riêng)",
        "Tổng chi phí vận hành (không gồm thưởng)",
        "Lợi nhuận vận hành (căn cứ thưởng)",
        "Lợi nhuận doanh nghiệp sau chia lợi nhuận nhân viên",
        "Đã hoàn chi hộ",
        "Đã chi tạm ứng",
        "Đã thu tạm ứng",
        "Còn phải hoàn",
        "Tạm ứng còn phải thu",
    ]);
    headings
}

fn write_headers(worksheet: &mut Worksheet, headings: &[&str]) -> Result<(), XlsxError> {
    write_headers_at(worksheet, 0, headings)
}

fn write_headers_at(worksheet: &mut Worksheet, row: u32, headings: &[&str]) -> Result<(), XlsxError> {
    let format: Format = header_format();
    for (column, heading) in headings.iter().enumerate() {
        worksheet.write_string_with_format(row, checked_column(column)?, *heading, &format)?;
    }
    Ok(())
}

fn write_financial_amounts(worksheet: &mut Worksheet, row: u32, amount: &FinancialAmounts) -> Result<(), XlsxError> {
    for (column, value) in [
        (1, amount.staffing_revenue),
        (2, amount.staffing_worker_cost),
        (3, amount.coordination_salary_cost),
        (4, amount.approved_business_expense),
        (5, amount.profit_share_cost),
        (6, amount.operating_cost),
        (7, amount.operating_profit),
        (8, amount.business_profit_after_profit_share),
        (9, amount.reimbursed_cash),
        (10, amount.salary_advance_disbursed),
        (11, amount.salary_advance_recovered),
        (12, amount.outstanding_expense_reimbursement),
        (13, amount.outstanding_salary_advance),
    ] {
        write_money_scaled(worksheet, row, column, value)?;
    }
    Ok(())
}

fn write_financial_line(
    worksheet: &mut Worksheet,
    row: u32,
    line: &OperatingFinancialLine,
    first_column: u16,
) -> Result<(), XlsxError> {
    let values: [&str; 13] = [
        &line.staffing_revenue,
        &line.staffing_worker_cost,
        &line.coordination_salary_cost,
        &line.approved_business_expense,
        &line.profit_share_cost,
        &line.operating_cost,
        &line.operating_profit,
        &line.business_profit_after_profit_share,
        &line.reimbursed_cash,
        &line.salary_advance_disbursed,
        &line.salary_advance_recovered,
        &line.outstanding_expense_reimbursement,
        &line.outstanding_salary_advance,
    ];
    let format: Format = money_format();
    for (offset, value) in values.iter().enumerate() {
        let column: u16 = first_column
            .checked_add(checked_column(offset)?)
            .ok_or(XlsxError::RowColumnLimitError)?;
        worksheet.write_number_with_format(row, column, excel_number(value)?, &format)?;
    }
    Ok(())
}

fn configure_money_columns(worksheet: &mut Worksheet, first: u16, last: u16) -> Result<(), XlsxError> {
    worksheet.set_column_width(first, 12.0)?;
    for column in first.saturating_add(1)..=last {
        worksheet.set_column_width(column, 19.0)?;
    }
    Ok(())
}

fn write_money_scaled(worksheet: &mut Worksheet, row: u32, column: u16, value: i128) -> Result<(), XlsxError> {
    let decimal: String = format_scaled(value);
    worksheet.write_number_with_format(row, column, excel_number(&decimal)?, &money_format())?;
    Ok(())
}

fn write_label_value(
    worksheet: &mut Worksheet,
    row: u32,
    label: &str,
    value: &str,
    label_format: &Format,
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(row, 0, label, label_format)?;
    worksheet.write_string(row, 1, value)?;
    Ok(())
}

fn parse_scaled(value: &str) -> Result<i128, XlsxError> {
    let trimmed: &str = value.trim();
    let (negative, unsigned): (bool, &str) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed),
    };
    let mut parts = unsigned.split('.');
    let whole: &str = parts.next().unwrap_or_default();
    let fraction: &str = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 4
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(XlsxError::ParameterError("invalid report decimal".to_owned()));
    }
    let whole_value: i128 = whole
        .parse()
        .map_err(|_| XlsxError::ParameterError("report decimal is too large".to_owned()))?;
    let fraction_value: i128 = format!("{fraction:0<4}")
        .parse()
        .map_err(|_| XlsxError::ParameterError("invalid report decimal fraction".to_owned()))?;
    let scaled: i128 = whole_value
        .checked_mul(MONEY_SCALE)
        .and_then(|amount| amount.checked_add(fraction_value))
        .ok_or_else(|| XlsxError::ParameterError("report decimal is too large".to_owned()))?;
    Ok(if negative { -scaled } else { scaled })
}

fn add_decimal(current: i128, value: &str) -> Result<i128, XlsxError> {
    current
        .checked_add(parse_scaled(value)?)
        .ok_or_else(|| XlsxError::ParameterError("report decimal total is too large".to_owned()))
}

fn format_scaled(value: i128) -> String {
    let negative: bool = value < 0;
    let absolute: i128 = value.abs();
    format!(
        "{}{}.{:04}",
        if negative { "-" } else { "" },
        absolute / MONEY_SCALE,
        absolute % MONEY_SCALE
    )
}

fn excel_number(value: &str) -> Result<f64, XlsxError> {
    if significant_digit_count(value) > 15 {
        return Err(XlsxError::ParameterError(
            "report amount exceeds Excel's exact numeric precision".to_owned(),
        ));
    }
    value
        .parse::<f64>()
        .map_err(|_| XlsxError::ParameterError("invalid report number".to_owned()))
}

fn significant_digit_count(value: &str) -> usize {
    let unsigned: &str = value.trim().trim_start_matches(['-', '+']);
    let mut parts = unsigned.split('.');
    let whole: &str = parts.next().unwrap_or_default().trim_start_matches('0');
    let fraction: &str = parts.next().unwrap_or_default().trim_end_matches('0');
    if whole.is_empty() {
        fraction.trim_start_matches('0').len()
    } else {
        whole.len() + fraction.len()
    }
}

fn integer_number(value: i64) -> Result<f64, XlsxError> {
    value
        .to_string()
        .parse::<f64>()
        .map_err(|_| XlsxError::ParameterError("invalid integer".to_owned()))
}

fn duration_number(seconds: i64) -> Result<f64, XlsxError> {
    integer_number(seconds).map(|value| value / 86_400.0)
}

fn checked_row(offset: usize, base: u32) -> Result<u32, XlsxError> {
    u32::try_from(offset)
        .ok()
        .and_then(|value| value.checked_add(base))
        .ok_or(XlsxError::RowColumnLimitError)
}

fn checked_column(value: usize) -> Result<u16, XlsxError> {
    u16::try_from(value).map_err(|_| XlsxError::RowColumnLimitError)
}

fn next_row(row: u32) -> Result<u32, XlsxError> {
    row.checked_add(1).ok_or(XlsxError::RowColumnLimitError)
}

fn report_title(kind: ReportExportKind) -> &'static str {
    match kind {
        ReportExportKind::OperatingFinancial => "BÁO CÁO TÀI CHÍNH VẬN HÀNH",
        ReportExportKind::Payroll => "BẢNG LƯƠNG NHÂN VIÊN",
    }
}

fn role_label(role: &RoleCode) -> &'static str {
    match role.as_str() {
        "tenant_owner" => "Chủ doanh nghiệp",
        "executive_manager" => "Quản lý điều hành",
        "branch_manager" => "Quản lý chi nhánh",
        "supervisor" => "Giám sát",
        "staff" => "Staff",
        _ => "Vai trò khác",
    }
}

fn title_format() -> Format {
    Format::new().set_bold().set_font_size(16.0).set_font_color(Color::Navy)
}

fn header_format() -> Format {
    Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::Navy)
        .set_align(FormatAlign::Center)
}

fn label_format() -> Format {
    Format::new().set_bold().set_font_color(Color::Navy)
}

fn money_format() -> Format {
    Format::new().set_num_format("#,##0.####;[Red]-#,##0.####")
}

fn duration_format() -> Format {
    Format::new().set_num_format("[h]:mm")
}

fn date_only_format() -> Format {
    Format::new().set_num_format("dd/mm/yyyy")
}

fn datetime_format() -> Format {
    Format::new().set_num_format("dd/mm/yyyy hh:mm")
}

fn warning_format() -> Format {
    Format::new()
        .set_bold()
        .set_font_color(Color::Red)
        .set_background_color("#FEE2E2")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use calamine::{Reader, Xlsx};

    use super::*;

    #[test]
    fn decimal_precision_guard_accepts_normal_money_and_rejects_unsafe_excel_amounts() {
        assert!(excel_number("12345678901.1234").is_ok());
        assert!(excel_number("123456789012.1234").is_err());
        assert_eq!(parse_scaled("123.45").ok(), Some(1_234_500));
        assert_eq!(format_scaled(-1_234_500), "-123.4500");
    }

    #[test]
    fn payroll_workbook_is_readable_and_has_expected_sheets() -> Result<(), Box<dyn std::error::Error>> {
        let metadata = ReportExportMetadata {
            tenant_name: "Công ty Demo".to_owned(),
            actor_username: "manager.demo".to_owned(),
            generated_at: Utc::now(),
            start_date: NaiveDate::from_ymd_opt(2026, 8, 1).ok_or("invalid start date")?,
            end_date: NaiveDate::from_ymd_opt(2026, 8, 31).ok_or("invalid end date")?,
        };
        let generated: GeneratedWorkbook = build_payroll_workbook(&metadata, &[], &[])?;
        let workbook: Xlsx<Cursor<Vec<u8>>> = Xlsx::new(Cursor::new(generated.bytes))?;
        assert_eq!(
            workbook.sheet_names(),
            &["Thông tin", "Tổng hợp", "Bảng lương", "Cảnh báo"]
        );
        Ok(())
    }
}
