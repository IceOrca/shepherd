# Shepherd

Shepherd is a highly customized, multi-tenant operations system for small and
medium staffing suppliers. It is not a standard ERP, HRM, attendance product,
or an implementation of a generic industry workflow. Its design follows the
real operating process of staffing companies whose work is urgent, informal,
and currently coordinated through spreadsheets and chat groups.

## Primary project target

Shepherd's primary target is to free supervisors and managers from repeatedly
copying staff attendance messages from Zalo, Telegram, or similar chat groups
into multi-sheet Excel workbooks.

The responsibility for producing staff-side work evidence moves to the staff:

1. At a customer workplace, a staff member selects the manager-maintained
   customer facility and presses **Start** for themselves and, when necessary,
   coworkers who cannot use a phone.
2. Staff press **Finish** after work. Shepherd records server-owned timestamps,
   the subject employee, and the acting account. It never trusts device time or
   silently infers attendance.
3. Supervisors no longer perform routine transcription of start, finish, and
   facility messages. They continue to dispatch workers and handle exceptions.

Urgent work without a pre-created shift is the default workflow because a
customer may request workers immediately while a supervisor is already
transporting them. Planned shifts remain an optional workflow when sufficient
lead time exists.

## Mandatory reconciliation

Staffing company records and customer records are deliberately independent.
Shepherd does not assume that staff evidence is correct merely because it was
recorded in the application, and customer systems do not currently synchronize
with Shepherd.

At the end of the day, a supervisor must compare staff-reported facility and
time against the customer's confirmation, bill, or time record:

- If both sources match, the result is ready for human review but is not
  automatically approved.
- If they differ, the supervisor contacts the customer, agrees on the true
  result, and records the conclusion with an audit reason.
- Only an explicit supervisor reconciliation locks the final facility, job,
  duration, billing rate, worker pay, margin, and payroll snapshot.

This separation of evidence and mandatory human conclusion is the heart of the
product. Scheduling, HR, payroll, authentication, and administration support
that workflow; they are not the product's primary purpose.

## Core product principles

- Optimize for the customer's actual staffing operation, not generic ERP
  conventions.
- Make urgent, staff-recorded work the shortest and most prominent workflow.
- Preserve who recorded work for whom through explicit self/peer provenance.
- Keep staff evidence immutable and customer evidence independent.
- Require a supervisor to reconcile every completed work report, including an
  exact match.
- Use PostgreSQL/server timestamps and tenant-scoped RLS as authoritative
  boundaries.
- Keep GPS disabled until the customer explicitly chooses to introduce it.

Detailed product invariants, architecture rules, API boundaries, and development
commands are maintained in [AGENTS.md](./AGENTS.md).
