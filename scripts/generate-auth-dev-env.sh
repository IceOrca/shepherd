#!/bin/sh

# Generate development-only Ed25519 access-token signing material and a
# separate ES256 administration key. The administration private key is consumed
# only by Shepherd; GoTrue receives only its public verification JWK.

set -eu

auth_output_path="deploy/supabase/dev/auth.env"
admin_output_path="deploy/supabase/dev/auth-admin.env"
force="${1:-}"

if [ -f ./.env ]; then
    set -a
    . ./.env
    set +a
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

require_setting AUTH_JWT_VALID_METHODS
require_setting AUTH_ADMIN_JWT_ALGORITHM
require_positive_integer AUTH_ADMIN_JWT_EXPIRY_SECS
require_positive_integer AUTH_ACCESS_JWT_ROTATION_INTERVAL_SECS
require_positive_integer AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS
require_positive_integer AUTH_ACCESS_JWT_KEY_OVERLAP_SECS

if [ "${AUTH_JWT_VALID_METHODS}" != "EdDSA,ES256" ]; then
    echo >&2 "AUTH_JWT_VALID_METHODS must be EdDSA,ES256"
    exit 2
fi
if [ "${AUTH_ADMIN_JWT_ALGORITHM}" != "ES256" ]; then
    echo >&2 "AUTH_ADMIN_JWT_ALGORITHM must be ES256"
    exit 2
fi
if [ "${AUTH_ADMIN_JWT_EXPIRY_SECS}" -gt 3600 ]; then
    echo >&2 "AUTH_ADMIN_JWT_EXPIRY_SECS must not exceed 3600"
    exit 2
fi

if { [ -e "${auth_output_path}" ] || [ -e "${admin_output_path}" ]; } && [ "${force}" != "--force" ]; then
    echo "Development Auth credentials already exist; pass --force to replace both key sets"
    exit 0
fi

mkdir -p "$(dirname "${auth_output_path}")"
umask 077

combined_temporary_path="$(mktemp)"
auth_temporary_path="$(mktemp)"
admin_temporary_path="$(mktemp)"
trap 'rm -f "${combined_temporary_path}" "${auth_temporary_path}" "${admin_temporary_path}"' EXIT HUP INT TERM

docker run --rm \
    -e AUTH_JWT_VALID_METHODS \
    -e AUTH_ADMIN_JWT_ALGORITHM \
    -e AUTH_ADMIN_JWT_EXPIRY_SECS \
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
const adminPublicJwk = adminPublicKey.export({ format: "jwk" });
const adminPrivatePem = adminPrivateKey.export({ format: "pem", type: "pkcs8" });
const adminKid = crypto.randomUUID();
const createdAt = Math.floor(Date.now() / 1000);
const provisioningFingerprintKey = crypto.randomBytes(32).toString("base64");

const signingKeys = [
  {
    kty: "OKP",
    kid: accessKid,
    use: "sig",
    key_ops: ["sign", "verify"],
    alg: "EdDSA",
    ext: true,
    crv: "Ed25519",
    x: accessKey.x,
    d: accessKey.d,
  },
  {
    kty: "EC",
    kid: adminKid,
    use: "sig",
    key_ops: ["verify"],
    alg: "ES256",
    ext: true,
    crv: "P-256",
    x: adminPublicJwk.x,
    y: adminPublicJwk.y,
  },
];

console.log(`GOTRUE_JWT_SECRET=${jwtSecret}`);
console.log(`GOTRUE_JWT_KEYS=${JSON.stringify(signingKeys)}`);
console.log(`GOTRUE_JWT_VALID_METHODS=${process.env.AUTH_JWT_VALID_METHODS}`);
console.log(`AUTH_ACCESS_JWT_ROTATION_INTERVAL_SECS=${process.env.AUTH_ACCESS_JWT_ROTATION_INTERVAL_SECS}`);
console.log(`AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS=${process.env.AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS}`);
console.log(`AUTH_ACCESS_JWT_KEY_OVERLAP_SECS=${process.env.AUTH_ACCESS_JWT_KEY_OVERLAP_SECS}`);
console.log(`AUTH_ACCESS_JWT_CURRENT_KID=${accessKid}`);
console.log(`AUTH_ACCESS_JWT_CURRENT_CREATED_AT=${createdAt}`);
console.log(`AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64=${Buffer.from(adminPrivatePem).toString("base64")}`);
console.log(`AUTH_ADMIN_JWT_KEY_ID=${adminKid}`);
console.log(`AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64=${provisioningFingerprintKey}`);
' > "${combined_temporary_path}"

sed -n '/^GOTRUE_/p; /^AUTH_ACCESS_/p' "${combined_temporary_path}" > "${auth_temporary_path}"
sed -n '/^AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64=/p; /^AUTH_ADMIN_JWT_KEY_ID=/p; /^AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64=/p' "${combined_temporary_path}" \
    > "${admin_temporary_path}"
mv "${auth_temporary_path}" "${auth_output_path}"
mv "${admin_temporary_path}" "${admin_output_path}"
chmod 600 "${auth_output_path}" "${admin_output_path}"
rm -f "${combined_temporary_path}"
trap - EXIT HUP INT TERM
echo "Generated Ed25519 access and ES256 administration keys for development"
echo "Recreate supabase-auth, server, and tenant-bootstrap consumers before use"
