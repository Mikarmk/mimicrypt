import { AppState, Contact } from "./model";

const STORAGE_KEY = "mimicrypt-desktop-state-v1";
const FALLBACK_EXPORT_ERROR = "Не удалось прочитать сохранённый профиль";

export const PROVIDER_PRESETS = {
  yandex: {
    label: "Yandex Mail",
    domainHint: "@yandex.ru",
    smtpHost: "smtp.yandex.ru",
    imapHost: "imap.yandex.ru",
  },
  mailru: {
    label: "Mail.ru",
    domainHint: "@mail.ru",
    smtpHost: "smtp.mail.ru",
    imapHost: "imap.mail.ru",
  },
  gmail: {
    label: "Gmail",
    domainHint: "@gmail.com",
    smtpHost: "smtp.gmail.com",
    imapHost: "imap.gmail.com",
  },
  outlook: {
    label: "Outlook",
    domainHint: "@outlook.com",
    smtpHost: "smtp.office365.com",
    imapHost: "outlook.office365.com",
  },
  generic: {
    label: "Other IMAP/SMTP",
    domainHint: "@example.com",
    smtpHost: "",
    imapHost: "",
  },
} as const;

function fingerprintFromEmail(email: string): string {
  const clean = email.toLowerCase();
  let hash = 0;
  for (let index = 0; index < clean.length; index += 1) {
    hash = (hash * 31 + clean.charCodeAt(index)) >>> 0;
  }
  return `${hash.toString(16).padStart(8, "0")}-${(hash ^ 0xa53f9b1e).toString(16).padStart(8, "0")}`;
}

function safetyNumber(localEmail: string, remoteEmail: string): string {
  const seed = `${localEmail}:${remoteEmail}`;
  let acc = 19;
  for (let index = 0; index < seed.length; index += 1) {
    acc = (acc * 131 + seed.charCodeAt(index)) % 1_000_000_000;
  }
  const s = acc.toString().padStart(9, "0");
  return `${s.slice(0, 3)}-${s.slice(3, 6)}-${s.slice(6, 9)}`;
}

export function createInitialState(): AppState {
  return {
    onboardingDone: false,
    profile: null,
    contacts: [],
    invites: [],
    messages: [],
    privacyMode: "balanced",
    telemetryEnabled: false,
    statusNote: "Device is the source of truth. Without encrypted export, device loss means access loss.",
  };
}

export function loadBrowserState(): AppState {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) return createInitialState();

  try {
    return JSON.parse(raw) as AppState;
  } catch {
    return createInitialState();
  }
}

export function saveBrowserState(state: AppState): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

export function isNativeShell(): boolean {
  return Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
}

async function invokeNative<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export async function loadState(): Promise<AppState> {
  if (!isNativeShell()) {
    return loadBrowserState();
  }

  try {
    const envelope = await invokeNative<{ stateJson: string } | null>("load_app_state");
    if (!envelope?.stateJson) return createInitialState();
    return JSON.parse(envelope.stateJson) as AppState;
  } catch {
    return loadBrowserState();
  }
}

export async function saveState(state: AppState): Promise<void> {
  saveBrowserState(state);

  if (!isNativeShell()) return;
  await invokeNative("save_app_state", {
    stateJson: JSON.stringify(state),
  });
}

export async function resetState(): Promise<AppState> {
  localStorage.removeItem(STORAGE_KEY);

  if (isNativeShell()) {
    await invokeNative("reset_app_state");
  }

  return createInitialState();
}

export function createContact(localEmail: string, email: string, note: string): Contact {
  return {
    id: crypto.randomUUID(),
    email,
    note,
    fingerprint: fingerprintFromEmail(email),
    safetyNumber: safetyNumber(localEmail, email),
    trustState: "unverified",
    identityPinnedAt: new Date().toISOString(),
  };
}

export function exportState(state: AppState): string {
  return btoa(unescape(encodeURIComponent(JSON.stringify(state))));
}

export function importState(encoded: string): AppState {
  try {
    return JSON.parse(decodeURIComponent(escape(atob(encoded)))) as AppState;
  } catch {
    throw new Error(FALLBACK_EXPORT_ERROR);
  }
}
