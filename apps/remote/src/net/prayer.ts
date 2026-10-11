import { KEYS, safeStorage } from "../lib/safeStorage";

export interface PrayerRequest {
  text: string;
  name: string;
}

type FetchLike = (input: string, init?: RequestInit) => Promise<Pick<Response, "ok" | "status">>;

const MAX_OUTBOX = 20;
export const MAX_PRAYER_CHARS = 500;

export function readOutbox(): PrayerRequest[] {
  const raw = safeStorage.getJson<unknown>(KEYS.prayerOutbox, []);
  return Array.isArray(raw)
    ? raw.filter(
        (x): x is PrayerRequest =>
          typeof x === "object" &&
          x !== null &&
          typeof (x as PrayerRequest).text === "string" &&
          typeof (x as PrayerRequest).name === "string",
      )
    : [];
}

function writeOutbox(items: PrayerRequest[]): void {
  if (items.length === 0) safeStorage.remove(KEYS.prayerOutbox);
  else safeStorage.setJson(KEYS.prayerOutbox, items.slice(-MAX_OUTBOX));
}

async function post(http: string, req: PrayerRequest, fetchFn: FetchLike): Promise<boolean> {
  try {
    const res = await fetchFn(`${http}/api/prayer`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        text: req.text.slice(0, MAX_PRAYER_CHARS),
        name: req.name.slice(0, 80),
      }),
    });
    return res.ok;
  } catch {
    return false;
  }
}

/**
 * Sends a prayer request to `POST /api/prayer`. When the host cannot be reached (or does not
 * expose that endpoint yet) the request is kept in a small local outbox and retried later with
 * `flushPrayers`, so a person's request is never lost silently.
 */
export async function submitPrayer(
  http: string,
  req: PrayerRequest,
  fetchFn: FetchLike = (i, init) => fetch(i, init),
): Promise<"sent" | "queued"> {
  const clean = { text: req.text.trim(), name: req.name.trim() };
  if (await post(http, clean, fetchFn)) return "sent";
  writeOutbox([...readOutbox(), clean]);
  return "queued";
}

/** Retries the queued requests in order; stops at the first failure. Returns how many were sent. */
export async function flushPrayers(
  http: string,
  fetchFn: FetchLike = (i, init) => fetch(i, init),
): Promise<number> {
  const items = readOutbox();
  let sent = 0;
  for (const it of items) {
    if (!(await post(http, it, fetchFn))) break;
    sent++;
  }
  if (sent > 0) writeOutbox(items.slice(sent));
  return sent;
}
