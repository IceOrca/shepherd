#!/bin/sh

# Generate development-only encryption and keyed-lookup material for employee
# citizen IDs. The output is ignored and created with mode 0600.

set -eu

output_path="deploy/shepherd/dev/hr-pii.env"
force="${1:-}"

if [ -e "${output_path}" ] && [ "${force}" != "--force" ]; then
    echo "Development HR PII keys already exist; pass --force to rotate them"
    exit 0
fi

mkdir -p "$(dirname "${output_path}")"
umask 077
temporary_path="$(mktemp)"
trap 'rm -f "${temporary_path}"' EXIT HUP INT TERM

docker run --rm node:24-alpine node -e '
const crypto = require("crypto");
const encryptionKey = crypto.randomBytes(32).toString("base64");
const lookupKey = crypto.randomBytes(32).toString("base64");
console.log("HR_CITIZEN_ID_ACTIVE_KEY_ID=v1");
console.log(`HR_CITIZEN_ID_ENCRYPTION_KEYS_JSON=${JSON.stringify({ v1: encryptionKey })}`);
console.log(`HR_CITIZEN_ID_LOOKUP_KEY_BASE64=${lookupKey}`);
' > "${temporary_path}"

mv "${temporary_path}" "${output_path}"
chmod 600 "${output_path}"
trap - EXIT HUP INT TERM
echo "Generated development HR citizen-ID protection keys"
