export const DEFAULT_SECRET_BYTES = 32;
export const MIN_SECRET_BYTES = 1;
export const MAX_SECRET_BYTES = 256;

export function parseSecretByteLength(value = DEFAULT_SECRET_BYTES) {
  const num = typeof value === "number" ? value : Number(String(value).trim());
  if (!Number.isInteger(num) || num < MIN_SECRET_BYTES || num > MAX_SECRET_BYTES) {
    throw new Error(
      `Secret length must be an integer between ${MIN_SECRET_BYTES} and ${MAX_SECRET_BYTES}`,
    );
  }
  return num;
}

export function encodeApiToken(bytes) {
  return encodeURIComponent(bytesToBase64(bytes));
}

export function encodePassword(bytes) {
  return bytesToBase64(bytes)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}

export function generateApiToken(byteLength = DEFAULT_SECRET_BYTES) {
  return encodeApiToken(randomBytes(parseSecretByteLength(byteLength)));
}

export function generatePassword(byteLength = DEFAULT_SECRET_BYTES) {
  return encodePassword(randomBytes(parseSecretByteLength(byteLength)));
}

function randomBytes(length) {
  const bytes = new Uint8Array(length);
  globalThis.crypto.getRandomValues(bytes);
  return bytes;
}

function bytesToBase64(bytes) {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return globalThis.btoa(binary);
}
