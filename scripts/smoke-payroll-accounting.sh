#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
accounts_file="${project_dir}/scripts/dev-auth-accounts.tsv"

owner_record="$(awk -F '\t' '$4 == "tenant_owner" { print $1 "\t" $6 "\t" $7; exit }' "${accounts_file}")"
if [[ -z "${owner_record}" ]]; then
  echo "No development tenant owner account was found." >&2
  exit 1
fi

IFS=$'\t' read -r smoke_tenant smoke_email smoke_password <<< "${owner_record}"
auth_payload="$(jq -nc \
  --arg email "${smoke_email}" \
  --arg password "${smoke_password}" \
  '{email: $email, password: $password}')"
auth_response="$(curl -fsS \
  -H "Content-Type: application/json" \
  -d "${auth_payload}" \
  "http://127.0.0.1:9999/token?grant_type=password")"
smoke_token="$(jq -er '.access_token' <<< "${auth_response}")"

smoke_branch="$(
  cd "${project_dir}"
  rtk docker compose exec -T postgres-db \
    psql -U postgresroot -d shepherd_dev -Atc \
    "SELECT id FROM branches WHERE tenant_id = '${smoke_tenant}'::uuid ORDER BY created_at LIMIT 1"
)"

common_headers=(
  -H "Authorization: Bearer ${smoke_token}"
  -H "X-Tenant-Id: ${smoke_tenant}"
  -H "X-Branch-Id: ${smoke_branch}"
)
range_start="$(date +%Y-%m-01)"
range_end="$(date -d "${range_start} +1 month -1 day" +%F)"
range_query="start_date=${range_start}&end_date=${range_end}"

operating="$(curl -fsS "${common_headers[@]}" \
  "http://127.0.0.1:8000/api/business/finance/operating-report?${range_query}")"
payroll="$(curl -fsS "${common_headers[@]}" \
  "http://127.0.0.1:8000/api/business/finance/payroll-report?${range_query}")"
salaries="$(curl -fsS "${common_headers[@]}" \
  "http://127.0.0.1:8000/api/business/finance/salary-configurations")"

jq -n \
  --argjson operating "${operating}" \
  --argjson payroll "${payroll}" \
  --argjson salaries "${salaries}" \
  '{
    operating: {
      start_date: $operating.start_date,
      end_date: $operating.end_date,
      currencies: ($operating.lines | length),
      totals: ($operating.lines[0] // {})
    },
    payroll: {
      employees: ($payroll.lines | length),
      blocking_overlaps: (($payroll.lines | map(.attendance_overlap_count) | add) // 0)
    },
    salary_configurations: ($salaries | length)
  }'
