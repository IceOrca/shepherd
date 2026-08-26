#!/bin/sh

# Generate new production key snippets without printing private material. Merge
# the first output into the protected Compose environment and the second into
# the mounted server.prod.env only after reviewing their variable names.

set -eu

auth_output_path="${1:-}"
admin_output_path="${2:-}"

if [ -z "${auth_output_path}" ] || [ -z "${admin_output_path}" ]; then
    echo >&2 "usage: $0 <production-auth-env-output> <production-server-admin-env-output>"
    exit 2
fi
if [ -e "${auth_output_path}" ] || [ -e "${admin_output_path}" ]; then
    echo >&2 "refusing to overwrite an existing production key output"
    exit 2
fi

require_setting() {
    variable_name="$1"
    eval "variable_value=\${${variable_name}:-}"
    if [ -z "${variable_value}" ]; then
        echo >&2 "${variable_name} is required"
        exit 2
    fi
}

require_positive_integer() {
    variable_name="$1"
    eval "variable_value=\${${variable_name}:-}"
    case "${variable_value}" in
        ''|*[!0-9]*|0)
            echo >&2 "${variable_name} must be a positive integer"
            exit 2
            ;;
    esac
}

for variable_name in \
    AUTH_PUBLIC_URL_PROD \
    AUTH_AUDIENCE_PROD \
    AUTH_JWT_VALID_METHODS_PROD \
    AUTH_ADMIN_JWT_ALGORITHM_PROD \
    AUTH_ADMIN_JWT_ROLE_PROD
do
    require_setting "${variable_name}"
done
require_positive_integer AUTH_ADMIN_JWT_EXPIRY_SECS_PROD
require_positive_integer AUTH_ACCESS_JWT_ROTATION_INTERVAL_SECS
require_positive_integer AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS
require_positive_integer AUTH_ACCESS_JWT_KEY_OVERLAP_SECS

if [ "${AUTH_JWT_VALID_METHODS_PROD}" != "EdDSA,ES256" ]; then
    echo >&2 "AUTH_JWT_VALID_METHODS_PROD must be EdDSA,ES256"
    exit 2
fi
if [ "${AUTH_ADMIN_JWT_ALGORITHM_PROD}" != "ES256" ]; then
    echo >&2 "AUTH_ADMIN_JWT_ALGORITHM_PROD must be ES256"
    exit 2
fi
if [ "${AUTH_ADMIN_JWT_EXPIRY_SECS_PROD}" -gt 3600 ]; then
    echo >&2 "AUTH_ADMIN_JWT_EXPIRY_SECS_PROD must not exceed 3600"
    exit 2
fi

mkdir -p "$(dirname "${auth_output_path}")" "$(dirname "${admin_output_path}")"
umask 077
combined_temporary_path="$(mktemp)"
auth_temporary_path="$(mktemp)"
admin_temporary_path="$(mktemp)"
trap 'rm -f "${combined_temporary_path}" "${auth_temporary_path}" "${admin_temporary_path}"' EXIT HUP INT TERM

docker run --rm \
    -e AUTH_PUBLIC_URL_PROD \
    -e AUTH_AUDIENCE_PROD \
    -e AUTH_JWT_VALID_METHODS_PROD \
    -e AUTH_ADMIN_JWT_ALGORITHM_PROD \
    -e AUTH_ADMIN_JWT_ROLE_PROD \
    -e AUTH_ADMIN_JWT_EXPIRY_SECS_PROD \
    -e AUTH_ACCESS_JWT_ROTATION_INTERVAL_SECS \
    -e AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS \
    -e AUTH_ACCESS_JWT_KEY_OVERLAP_SECS \
    node:24-alpine node -e '
const crypto = require("crypto");

const jwtSecret = crypto.randomBytes(48).toString("base64url");
const { privateKey: accessPrivateKey } = crypto.generateKeyPairSync("ed25519");
const accessKey = accessPrivateKey.export({ format: "jwk" });
const accessKid = crypto.randomUUID();
const { privateKey: adminPrivateKey, publicKey: adminPublicKey } =
  crypto.generateKeyPairSync("ec", { namedCurve: "prime256v1" });
const adminPublic = adminPublicKey.export({ format: "jwk" });
const adminPrivatePem = adminPrivateKey.export({ format: "pem", type: "pkcs8" });
const adminKid = crypto.randomUUID();
const createdAt = Math.floor(Date.now() / 1000);
const keys = [
  {
    kty: "OKP", kid: accessKid, use: "sig", key_ops: ["sign", "verify"],
    alg: "EdDSA", ext: true, crv: "Ed25519", x: accessKey.x, d: accessKey.d,
  },
  {
    kty: "EC", kid: adminKid, use: "sig", key_ops: ["verify"],
    alg: "ES256", ext: true, crv: "P-256", x: adminPublic.x, y: adminPublic.y,
  },
];

console.log(`AUTH_JWT_SECRET_PROD=${jwtSecret}`);
console.log(`AUTH_JWT_KEYS_PROD=${JSON.stringify(keys)}`);
console.log(`AUTH_JWT_VALID_METHODS_PROD=${process.env.AUTH_JWT_VALID_METHODS_PROD}`);
console.log(`AUTH_ACCESS_JWT_ROTATION_INTERVAL_SECS=${process.env.AUTH_ACCESS_JWT_ROTATION_INTERVAL_SECS}`);
console.log(`AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS=${process.env.AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS}`);
console.log(`AUTH_ACCESS_JWT_KEY_OVERLAP_SECS=${process.env.AUTH_ACCESS_JWT_KEY_OVERLAP_SECS}`);
console.log(`AUTH_ACCESS_JWT_CURRENT_KID=${accessKid}`);
console.log(`AUTH_ACCESS_JWT_CURRENT_CREATED_AT=${createdAt}`);
console.log(`AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64=${Buffer.from(adminPrivatePem).toString("base64")}`);
console.log(`AUTH_ADMIN_JWT_KEY_ID=${adminKid}`);
' > "${combined_temporary_path}"

sed -n '/^AUTH_JWT_/p; /^AUTH_ACCESS_/p' "${combined_temporary_path}" > "${auth_temporary_path}"
sed -n '/^AUTH_ADMIN_JWT_/p' "${combined_temporary_path}" > "${admin_temporary_path}"
mv "${auth_temporary_path}" "${auth_output_path}"
mv "${admin_temporary_path}" "${admin_output_path}"
chmod 600 "${auth_output_path}" "${admin_output_path}"
rm -f "${combined_temporary_path}"
trap - EXIT HUP INT TERM
echo "Generated protected production Auth and server-admin key snippets"
