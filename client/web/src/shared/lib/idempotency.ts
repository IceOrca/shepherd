interface IdempotencyAttempt {
  keysByPayload: Map<string, string>;
}

export interface IdempotencyAttemptRef {
  current: IdempotencyAttempt | null;
}

export function idempotencyKeyForPayload(
  attemptRef: IdempotencyAttemptRef,
  payload: unknown,
): string {
  const payloadSignature: string | undefined = JSON.stringify(payload);
  if (payloadSignature === undefined) {
    throw new Error("idempotent mutation payload cannot be serialized");
  }
  if (attemptRef.current === null) {
    attemptRef.current = { keysByPayload: new Map<string, string>() };
  }
  const attempt: IdempotencyAttempt | null = attemptRef.current;
  if (attempt === null) {
    throw new Error("idempotent mutation attempt was not initialized");
  }
  const existingKey: string | undefined = attempt.keysByPayload.get(payloadSignature);
  if (existingKey !== undefined) {
    return existingKey;
  }
  const newKey: string = crypto.randomUUID();
  attempt.keysByPayload.set(payloadSignature, newKey);
  return newKey;
}

export function clearIdempotencyAttempt(attemptRef: IdempotencyAttemptRef): void {
  attemptRef.current = null;
}
