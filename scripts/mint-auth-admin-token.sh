#!/bin/sh

# Mint one short-lived ES256 service credential for a direct GoTrue /admin call.
# Required settings are intentionally environment-only; the token and private
# key are never written by this helper.

set -eu

require_setting() {
    variable_name="$1"
    eval "variable_value=\${${variable_name}:-}"
    if [ -z "${variable_value}" ]; then
        echo >&2 "${variable_name} is required"
        exit 2
    fi
}

for variable_name in \
    AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64 \
    AUTH_ADMIN_JWT_KEY_ID \
    AUTH_ADMIN_JWT_ALGORITHM \
    AUTH_ADMIN_JWT_ROLE \
    AUTH_ADMIN_JWT_ISSUER \
    AUTH_ADMIN_JWT_AUDIENCE \
    AUTH_ADMIN_JWT_EXPIRY_SECS
do
    require_setting "${variable_name}"
done

if [ "${AUTH_ADMIN_JWT_ALGORITHM}" != "ES256" ]; then
    echo >&2 "AUTH_ADMIN_JWT_ALGORITHM must be ES256"
    exit 2
fi
case "${AUTH_ADMIN_JWT_EXPIRY_SECS}" in
    ''|*[!0-9]*|0)
        echo >&2 "AUTH_ADMIN_JWT_EXPIRY_SECS must be a positive integer"
        exit 2
        ;;
esac
if [ "${AUTH_ADMIN_JWT_EXPIRY_SECS}" -gt 3600 ]; then
    echo >&2 "AUTH_ADMIN_JWT_EXPIRY_SECS must not exceed 3600"
    exit 2
fi

docker run --rm \
    -e AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64 \
    -e AUTH_ADMIN_JWT_KEY_ID \
    -e AUTH_ADMIN_JWT_ALGORITHM \
    -e AUTH_ADMIN_JWT_ROLE \
    -e AUTH_ADMIN_JWT_ISSUER \
    -e AUTH_ADMIN_JWT_AUDIENCE \
    -e AUTH_ADMIN_JWT_EXPIRY_SECS \
    node:24-alpine node -e '
const crypto = require("crypto");

const encode = (value) => Buffer.from(JSON.stringify(value)).toString("base64url");
const privateKey = crypto.createPrivateKey({
  key: Buffer.from(process.env.AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64, "base64"),
  format: "pem",
});
if (privateKey.asymmetricKeyType !== "ec" ||
    privateKey.asymmetricKeyDetails?.namedCurve !== "prime256v1") {
  throw new Error("AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64 must contain an ES256 P-256 private key");
}
const now = Math.floor(Date.now() / 1000);
const unsigned = [
  encode({
    alg: process.env.AUTH_ADMIN_JWT_ALGORITHM,
    kid: process.env.AUTH_ADMIN_JWT_KEY_ID,
    typ: "JWT",
  }),
  encode({
    role: process.env.AUTH_ADMIN_JWT_ROLE,
    iss: process.env.AUTH_ADMIN_JWT_ISSUER,
    aud: process.env.AUTH_ADMIN_JWT_AUDIENCE,
    iat: now,
    exp: now + Number(process.env.AUTH_ADMIN_JWT_EXPIRY_SECS),
  }),
].join(".");
const signature = crypto.sign("sha256", Buffer.from(unsigned), {
  key: privateKey,
  dsaEncoding: "ieee-p1363",
});
process.stdout.write(`${unsigned}.${signature.toString("base64url")}\n`);
'
