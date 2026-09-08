// Development-only end-to-end checks. Secrets are read locally and never printed.
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";

const credentials = Object.fromEntries(readFileSync("deploy/shepherd/dev/system-admin.env", "utf8").split(/\r?\n/)
  .filter((line) => line && !line.startsWith("#")).map((line) => {
    const index = line.indexOf("=");
    return [line.slice(0, index), line.slice(index + 1).replace(/^(['"])(.*)\1$/, "$2")];
  }));
const authUrl = "http://127.0.0.1:9999";
const apiUrl = "http://127.0.0.1:8000/api";
const tenantId = randomUUID();
const key = randomUUID();
const suffix = tenantId.slice(0, 8);
const ownerEmail = `platform-smoke-${suffix}@example.test`;
const oldPassword = `Test-${randomUUID()}-aA1!`;
const newPassword = `Changed-${randomUUID()}-aA1!`;
let ownerSubject;
let assertions = 0;
function check(condition, description) {
  if (!condition) throw new Error(description);
  assertions += 1;
}
async function call(url, { token, method = "GET", body, headers = {} } = {}) {
  const response = await fetch(url, {
    method, headers: { "Content-Type": "application/json", ...(token ? { Authorization: `Bearer ${token}` } : {}), ...headers },
    ...(body ? { body: JSON.stringify(body) } : {}), signal: AbortSignal.timeout(60000),
  });
  const text = await response.text();
  let data = null;
  try { data = text ? JSON.parse(text) : null; } catch { /* Status assertions identify failure without leaking bodies. */ }
  return { status: response.status, data };
}
async function login(email, password) {
  const result = await call(`${authUrl}/token?grant_type=password`, { method: "POST", body: { email, password } });
  check(result.status === 200, "Password sign-in failed");
  return result.data;
}
const adminToken = execFileSync("sh", ["-c", "set -a; . ./.env; . ./deploy/supabase/dev/auth-admin.env; AUTH_ADMIN_JWT_ISSUER=$AUTH_ISSUER_URL; AUTH_ADMIN_JWT_AUDIENCE=$AUTH_AUDIENCE; export AUTH_ADMIN_JWT_ISSUER AUTH_ADMIN_JWT_AUDIENCE; sh scripts/mint-auth-admin-token.sh"], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
const started = execFileSync("docker", ["inspect", "--format", "{{.State.StartedAt}}", "shepherd-server"], { encoding: "utf8" }).trim();
try {
  const admin = await login(credentials.SYSTEM_ADMIN_EMAIL, credentials.SYSTEM_ADMIN_PASSWORD);
  const profile = await call(`${apiUrl}/platform/session`, { token: admin.access_token });
  check(profile.status === 200 && profile.data.administrator !== null, "Admin session was not recognized");
  check((await call(`${apiUrl}/platform/log-level`)).status === 401, "Anonymous platform request was not rejected");
  for (const role of ["tenant_owner", "executive_manager", "branch_manager", "supervisor", "staff"]) {
    const row = readFileSync("scripts/dev-auth-accounts.tsv", "utf8").split(/\r?\n/).map((line) => line.split("\t")).find((cells) => cells[3] === role);
    const identity = await login(row[5], row[6]);
    const session = await call(`${apiUrl}/platform/session`, { token: identity.access_token });
    check(session.data.administrator === null, "Tenant role gained platform authority");
    check((await call(`${apiUrl}/platform/log-level`, { token: identity.access_token })).status === 403, "Tenant role could access platform logging");
    check((await call(`${apiUrl}/platform/tenants`, { token: identity.access_token, method: "POST", body: {} })).status === 403, "Tenant role could bootstrap a tenant");
  }
  for (const level of ["trace", "info", "debug"]) {
    const result = await call(`${apiUrl}/platform/log-level`, { token: admin.access_token, method: "PUT", body: { level } });
    check(result.status === 200 && result.data.filter.includes(`shepherd=${level}`), "Log level did not apply");
    const current = await call(`${apiUrl}/platform/log-level`, { token: admin.access_token });
    check(current.data.filter === result.data.filter, "Live log filter did not persist in process");
  }
  check((await call(`${apiUrl}/platform/log-level`, { token: admin.access_token, method: "PUT", body: { level: "debug,hyper=trace" } })).status === 422, "Arbitrary logging directives were accepted");
  const request = { tenant_id: tenantId, tenant_slug: `test-platform-${suffix}`, tenant_display_name: "Isolated platform smoke test", idempotency_key: key, owners: [{ username: `owner_${suffix}`, email: ownerEmail, password: oldPassword }] };
  const parallel = await Promise.all([1, 2].map(() => call(`${apiUrl}/platform/tenants`, { token: admin.access_token, method: "POST", body: request })));
  check(parallel.every((item) => item.status === 200 || item.status === 409), "Concurrent bootstrap returned an unexpected status");
  const first = parallel.find((item) => item.status === 200 && !item.data.replayed) ?? parallel[0];
  check(first.status === 200 && first.data.tenant_id === tenantId && !first.data.replayed, `Tenant bootstrap failed with status ${first.status}`);
  const replay = await call(`${apiUrl}/platform/tenants`, { token: admin.access_token, method: "POST", body: request });
  check(replay.status === 200 && replay.data.replayed, "Completed bootstrap replay did not return original result");
  const changed = await call(`${apiUrl}/platform/tenants`, { token: admin.access_token, method: "POST", body: { ...request, tenant_display_name: "Changed input" } });
  check(changed.status === 409, "Changed bootstrap input reused an idempotency key");
  const owner = await login(ownerEmail, oldPassword);
  ownerSubject = owner.user.id;
  const memberships = await call(`${apiUrl}/tenants`, { token: owner.access_token });
  check(memberships.status === 200 && memberships.data.some((item) => item.tenant_id === tenantId), "First owner has no tenant membership");
  const ownerProfile = await call(`${apiUrl}/me`, { token: owner.access_token, headers: { "X-Tenant-Id": tenantId } });
  check(ownerProfile.status === 200 && ownerProfile.data.primary_role === "tenant_owner", "First owner cannot load a profile without branches");
  const branch = await call(`${apiUrl}/business/branches`, { token: owner.access_token, method: "POST", headers: { "X-Tenant-Id": tenantId }, body: { code: "test-first", name: "Test first branch", time_zone: "Asia/Ho_Chi_Minh" } });
  check(branch.status === 201 || branch.status === 200, "First owner cannot create the first branch");
  check((await call(`${authUrl}/user`, { method: "PUT", body: { current_password: oldPassword, password: newPassword } })).status === 401, "Anonymous password change was accepted");
  const missing = await call(`${authUrl}/user`, { token: owner.access_token, method: "PUT", body: { password: newPassword } });
  check(missing.status === 400 && missing.data.error_code === "current_password_required", "Provider did not require the old password");
  const wrong = await call(`${authUrl}/user`, { token: owner.access_token, method: "PUT", body: { current_password: "wrong-current-password", password: newPassword } });
  check(wrong.status === 400 && ["current_password_invalid", "current_password_mismatch"].includes(wrong.data.error_code), `Wrong-password check returned status=${wrong.status}, code=${wrong.data?.error_code}`);
  const change = await call(`${authUrl}/user`, { token: owner.access_token, method: "PUT", body: { current_password: oldPassword, password: newPassword } });
  check(change.status === 200, "Valid password change failed");
  check((await call(`${authUrl}/token?grant_type=password`, { method: "POST", body: { email: ownerEmail, password: oldPassword } })).status === 400, "Old password still signs in");
  await login(ownerEmail, newPassword);
  const logs = execFileSync("docker", ["compose", "logs", "--tail", "2500", "server", "supabase-auth"], { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
  check(![oldPassword, newPassword, credentials.SYSTEM_ADMIN_PASSWORD].some((secret) => logs.includes(secret)), "A password appeared in service logs");
  const ended = execFileSync("docker", ["inspect", "--format", "{{.State.StartedAt}}", "shepherd-server"], { encoding: "utf8" }).trim();
  check(started === ended, "Server restarted during log-level changes");
  console.log(`Platform and password smoke checks passed (${assertions} assertions).`);
} catch (error) {
  console.error(`Smoke assertion failed: ${error.message}`);
  throw error;
} finally {
  // Only this generated test tenant is removed; append-only guards are restored
  // in the same cleanup transaction, following the repository fixture pattern.
  const cleanup = `BEGIN;
    ALTER TABLE access_control_audit_log DISABLE TRIGGER access_control_audit_log_immutable;
    DELETE FROM access_control_audit_log WHERE tenant_id = '${tenantId}';
    ALTER TABLE access_control_audit_log ENABLE TRIGGER access_control_audit_log_immutable;
    DELETE FROM accounts WHERE tenant_id = '${tenantId}';
    DELETE FROM branches WHERE tenant_id = '${tenantId}';
    DELETE FROM tenants WHERE id = '${tenantId}';
    DELETE FROM platform_tenant_bootstrap_requests WHERE idempotency_key = '${key}';
    COMMIT;`;
  try {
    execFileSync("docker", ["compose", "exec", "-T", "postgres-db", "psql", "-U", "postgresroot", "-d", "shepherd_dev", "-v", "ON_ERROR_STOP=1"], { input: cleanup, stdio: ["pipe", "pipe", "pipe"] });
  } catch (error) {
    console.error(`Temporary tenant cleanup failed: ${tenantId}`);
    throw new Error("Temporary database cleanup failed", { cause: error });
  } finally {
    if (!ownerSubject) {
      const users = await call(`${authUrl}/admin/users?filter=${encodeURIComponent(ownerEmail)}`, { token: adminToken });
      ownerSubject = users.data?.users?.find((user) => user.email === ownerEmail)?.id;
    }
    if (ownerSubject) {
      const removed = await call(`${authUrl}/admin/users/${encodeURIComponent(ownerSubject)}`, { token: adminToken, method: "DELETE" });
      check(removed.status === 200, "Temporary provider identity cleanup failed");
    }
  }
  console.log("Temporary tenant and provider identity removed; system administration audit retained.");
}
