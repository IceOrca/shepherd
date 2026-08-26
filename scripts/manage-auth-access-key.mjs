import crypto from "node:crypto";
import fs from "node:fs";

const [operation, keysVariable, forceFlag] = process.argv.slice(2);
const force = forceFlag === "--force";
const allowedOperations = new Set(["status", "prepare", "activate", "retire"]);

if (!allowedOperations.has(operation) || !/^[A-Z][A-Z0-9_]*$/.test(keysVariable ?? "")) {
  throw new Error("usage: manage-auth-access-key.mjs <status|prepare|activate|retire> <KEYS_VARIABLE> [--force]");
}
if (forceFlag && !force) {
  throw new Error("the only supported optional flag is --force");
}

const original = fs.readFileSync(0, "utf8");
const lines = original.replace(/\n$/, "").split("\n");
const values = new Map();
for (const line of lines) {
  const match = /^([A-Z][A-Z0-9_]*)=(.*)$/.exec(line);
  if (match) {
    values.set(match[1], match[2]);
  }
}

const required = (name) => {
  const value = values.get(name);
  if (!value) {
    throw new Error(`${name} is required in the Auth environment file`);
  }
  return value;
};
const positiveInteger = (name) => {
  const value = Number(required(name));
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return value;
};
const parseTimestamp = (name) => {
  const value = positiveInteger(name);
  if (value > Math.floor(Date.now() / 1000) + 300) {
    throw new Error(`${name} must not be in the future`);
  }
  return value;
};
const parseKeys = () => {
  let keys;
  try {
    keys = JSON.parse(required(keysVariable));
  } catch (error) {
    throw new Error(`${keysVariable} must be a JSON array: ${error.message}`);
  }
  if (!Array.isArray(keys)) {
    throw new Error(`${keysVariable} must be a JSON array`);
  }
  return keys;
};
const isSigner = (key) => Array.isArray(key.key_ops) && key.key_ops.includes("sign");
const publicEd25519 = (key) => ({
  kty: "OKP",
  kid: key.kid,
  use: "sig",
  key_ops: ["verify"],
  alg: "EdDSA",
  ext: true,
  crv: "Ed25519",
  x: key.x,
});
const privateEd25519 = (key, kid) => ({
  kty: "OKP",
  kid,
  use: "sig",
  key_ops: ["sign", "verify"],
  alg: "EdDSA",
  ext: true,
  crv: "Ed25519",
  x: key.x,
  d: key.d,
});
const setValue = (name, value) => values.set(name, String(value));
const deleteValue = (name) => values.delete(name);
const writeEnvironment = () => {
  const emitted = new Set();
  const output = lines
    .filter((line) => {
      const match = /^([A-Z][A-Z0-9_]*)=/.exec(line);
      return !match || values.has(match[1]);
    })
    .map((line) => {
      const match = /^([A-Z][A-Z0-9_]*)=/.exec(line);
      if (!match) {
        return line;
      }
      emitted.add(match[1]);
      return `${match[1]}=${values.get(match[1])}`;
    });
  for (const [name, value] of values) {
    if (!emitted.has(name)) {
      output.push(`${name}=${value}`);
    }
  }
  process.stdout.write(`${output.join("\n")}\n`);
};

let keys = parseKeys();
const signers = keys.filter(isSigner);
if (signers.length !== 1 || signers[0].alg !== "EdDSA") {
  throw new Error("the key set must contain exactly one EdDSA signing key");
}
const currentKid = required("AUTH_ACCESS_JWT_CURRENT_KID");
if (signers[0].kid !== currentKid) {
  throw new Error("AUTH_ACCESS_JWT_CURRENT_KID does not identify the active EdDSA signer");
}

const now = Math.floor(Date.now() / 1000);
const rotationInterval = positiveInteger("AUTH_ACCESS_JWT_ROTATION_INTERVAL_SECS");
const currentCreatedAt = parseTimestamp("AUTH_ACCESS_JWT_CURRENT_CREATED_AT");
const dueAt = currentCreatedAt + rotationInterval;

if (operation === "status") {
  const state = values.has("AUTH_ACCESS_JWT_STANDBY_KID")
    ? "standby"
    : values.has("AUTH_ACCESS_JWT_PREVIOUS_KID")
      ? "overlap"
      : now >= dueAt
        ? "due"
        : "current";
  process.stderr.write(
    `Access JWT key state=${state} current_kid=${currentKid} due_at=${new Date(dueAt * 1000).toISOString()}\n`,
  );
  writeEnvironment();
  process.exit(0);
}

if (operation === "prepare") {
  if (values.has("AUTH_ACCESS_JWT_STANDBY_KID") || values.has("AUTH_ACCESS_JWT_PREVIOUS_KID")) {
    throw new Error("finish the existing standby/overlap rotation before preparing another key");
  }
  if (now < dueAt && !force) {
    throw new Error(
      `rotation is not due until ${new Date(dueAt * 1000).toISOString()}; use --force for compromise or a planned early rotation`,
    );
  }
  const { privateKey } = crypto.generateKeyPairSync("ed25519");
  const rawKey = privateKey.export({ format: "jwk" });
  const standbyKid = crypto.randomUUID();
  const standbyPrivate = privateEd25519(rawKey, standbyKid);
  keys.push(publicEd25519(standbyPrivate));
  setValue(keysVariable, JSON.stringify(keys));
  setValue("AUTH_ACCESS_JWT_STANDBY_KID", standbyKid);
  setValue("AUTH_ACCESS_JWT_STANDBY_CREATED_AT", now);
  setValue(
    "AUTH_ACCESS_JWT_STANDBY_PRIVATE_JWK_BASE64",
    Buffer.from(JSON.stringify(standbyPrivate)).toString("base64"),
  );
  process.stderr.write(`Prepared Ed25519 standby key ${standbyKid}; recreate GoTrue to publish it before activation.\n`);
}

if (operation === "activate") {
  const standbyKid = required("AUTH_ACCESS_JWT_STANDBY_KID");
  const standbyCreatedAt = parseTimestamp("AUTH_ACCESS_JWT_STANDBY_CREATED_AT");
  const propagation = positiveInteger("AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS");
  const activationAllowedAt = standbyCreatedAt + propagation;
  if (now < activationAllowedAt && !force) {
    throw new Error(
      `standby propagation does not end until ${new Date(activationAllowedAt * 1000).toISOString()}; use --force only after independently confirming every verifier has the key`,
    );
  }
  const standbyPrivate = JSON.parse(
    Buffer.from(required("AUTH_ACCESS_JWT_STANDBY_PRIVATE_JWK_BASE64"), "base64").toString("utf8"),
  );
  if (standbyPrivate.kid !== standbyKid || standbyPrivate.alg !== "EdDSA" || !isSigner(standbyPrivate)) {
    throw new Error("the stored standby private JWK does not match AUTH_ACCESS_JWT_STANDBY_KID");
  }
  const standbyIndex = keys.findIndex((key) => key.kid === standbyKid);
  const currentIndex = keys.findIndex((key) => key.kid === currentKid);
  if (standbyIndex < 0 || currentIndex < 0) {
    throw new Error("the current or standby key is missing from the configured key set");
  }
  keys[currentIndex] = publicEd25519(keys[currentIndex]);
  keys[standbyIndex] = standbyPrivate;
  setValue(keysVariable, JSON.stringify(keys));
  setValue("AUTH_ACCESS_JWT_PREVIOUS_KID", currentKid);
  setValue("AUTH_ACCESS_JWT_ROTATION_ACTIVATED_AT", now);
  setValue("AUTH_ACCESS_JWT_CURRENT_KID", standbyKid);
  setValue("AUTH_ACCESS_JWT_CURRENT_CREATED_AT", standbyCreatedAt);
  deleteValue("AUTH_ACCESS_JWT_STANDBY_KID");
  deleteValue("AUTH_ACCESS_JWT_STANDBY_CREATED_AT");
  deleteValue("AUTH_ACCESS_JWT_STANDBY_PRIVATE_JWK_BASE64");
  process.stderr.write(
    `Activated Ed25519 key ${standbyKid}; recreate GoTrue and retain previous key ${currentKid} through the overlap.\n`,
  );
}

if (operation === "retire") {
  const previousKid = required("AUTH_ACCESS_JWT_PREVIOUS_KID");
  const activatedAt = parseTimestamp("AUTH_ACCESS_JWT_ROTATION_ACTIVATED_AT");
  const overlap = positiveInteger("AUTH_ACCESS_JWT_KEY_OVERLAP_SECS");
  const retireAt = activatedAt + overlap;
  if (now < retireAt && !force) {
    throw new Error(
      `verification overlap does not end until ${new Date(retireAt * 1000).toISOString()}; use --force only for emergency revocation`,
    );
  }
  const previous = keys.find((key) => key.kid === previousKid);
  if (!previous || isSigner(previous)) {
    throw new Error("AUTH_ACCESS_JWT_PREVIOUS_KID must identify a verification-only key");
  }
  keys = keys.filter((key) => key.kid !== previousKid);
  setValue(keysVariable, JSON.stringify(keys));
  deleteValue("AUTH_ACCESS_JWT_PREVIOUS_KID");
  deleteValue("AUTH_ACCESS_JWT_ROTATION_ACTIVATED_AT");
  process.stderr.write(`Retired previous Ed25519 verification key ${previousKid}; recreate GoTrue to revoke it.\n`);
}

writeEnvironment();
