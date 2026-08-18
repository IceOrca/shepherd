#!/bin/sh

# Generate private signing configuration for the development Supabase Auth
# container plus a separate service-role token consumed only by Shepherd.
# Both outputs are gitignored and created with mode 0600.

set -eu

auth_output_path="deploy/supabase/dev/auth.env"
admin_output_path="deploy/supabase/dev/auth-admin.env"
force="${1:-}"

mkdir -p "$(dirname "${auth_output_path}")"
umask 077

generate_admin_token() {
    docker run --rm -e JWT_SECRET node:24-alpine node -e '
const crypto = require("crypto");
const now = Math.floor(Date.now() / 1000);
const encode = (value) => Buffer.from(JSON.stringify(value)).toString("base64url");
const unsigned = [
  encode({ alg: "HS256", typ: "JWT" }),
  encode({ role: "service_role", iss: "supabase", iat: now, exp: now + 315360000 }),
].join(".");
const signature = crypto
  .createHmac("sha256", process.env.JWT_SECRET)
  .update(unsigned)
  .digest("base64url");
console.log(`AUTH_ADMIN_TOKEN=${unsigned}.${signature}`);
'
}

if [ -e "${auth_output_path}" ] && [ "${force}" != "--force" ]; then
    if [ -e "${admin_output_path}" ]; then
        echo "Development Auth credentials already exist; pass --force to rotate them"
        exit 0
    fi

    JWT_SECRET="$(sed -n 's/^GOTRUE_JWT_SECRET=//p' "${auth_output_path}")"
    if [ -z "${JWT_SECRET}" ]; then
        echo "${auth_output_path} does not contain GOTRUE_JWT_SECRET" >&2
        exit 1
    fi
    export JWT_SECRET
    admin_temporary_path="$(mktemp)"
    trap 'rm -f "${admin_temporary_path}"' EXIT
    generate_admin_token > "${admin_temporary_path}"
    mv "${admin_temporary_path}" "${admin_output_path}"
    chmod 600 "${admin_output_path}"
    trap - EXIT
    echo "Generated ${admin_output_path} from the existing development signing secret"
    exit 0
fi

combined_temporary_path="$(mktemp)"
auth_temporary_path="$(mktemp)"
admin_temporary_path="$(mktemp)"
trap 'rm -f "${combined_temporary_path}" "${auth_temporary_path}" "${admin_temporary_path}"' EXIT

docker run --rm node:24-alpine node -e '
const crypto = require("crypto");
const jwtSecret = crypto.randomBytes(48).toString("base64url");
const { privateKey } = crypto.generateKeyPairSync("ed25519");
const key = privateKey.export({ format: "jwk" });
const kid = crypto.randomUUID();
const signingKeys = [
  {
    kty: "OKP",
    kid,
    use: "sig",
    key_ops: ["sign", "verify"],
    alg: "EdDSA",
    ext: true,
    crv: "Ed25519",
    x: key.x,
    d: key.d,
  },
  {
    kty: "oct",
    k: Buffer.from(jwtSecret).toString("base64url"),
    alg: "HS256",
  },
];
const now = Math.floor(Date.now() / 1000);
const encode = (value) => Buffer.from(JSON.stringify(value)).toString("base64url");
const unsigned = [
  encode({ alg: "HS256", typ: "JWT" }),
  encode({ role: "service_role", iss: "supabase", iat: now, exp: now + 315360000 }),
].join(".");
const signature = crypto.createHmac("sha256", jwtSecret).update(unsigned).digest("base64url");

console.log(`GOTRUE_JWT_SECRET=${jwtSecret}`);
console.log(`GOTRUE_JWT_KEYS=${JSON.stringify(signingKeys)}`);
console.log(`AUTH_ADMIN_TOKEN=${unsigned}.${signature}`);
' > "${combined_temporary_path}"

sed '/^AUTH_ADMIN_TOKEN=/d' "${combined_temporary_path}" > "${auth_temporary_path}"
sed -n '/^AUTH_ADMIN_TOKEN=/p' "${combined_temporary_path}" > "${admin_temporary_path}"
mv "${auth_temporary_path}" "${auth_output_path}"
mv "${admin_temporary_path}" "${admin_output_path}"
chmod 600 "${auth_output_path}" "${admin_output_path}"
rm -f "${combined_temporary_path}"
trap - EXIT
echo "Generated ${auth_output_path} and ${admin_output_path}"
