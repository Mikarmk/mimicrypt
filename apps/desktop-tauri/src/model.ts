export type TrustState = "unverified" | "verified" | "blocked";
export type DeliveryState =
  | "queued"
  | "smtp_accepted"
  | "fetched_by_peer"
  | "decrypted_by_peer"
  | "read_by_peer"
  | "failed";

export interface AccountProfile {
  id: string;
  email: string;
  displayName: string;
  mailboxMode: "yandex" | "mailru" | "gmail" | "outlook" | "generic";
  smtpHost: string;
  imapHost: string;
  dedicatedMailboxRecommended: boolean;
}

export interface Contact {
  id: string;
  email: string;
  fingerprint: string;
  safetyNumber: string;
  trustState: TrustState;
  identityPinnedAt: string;
  note: string;
}

export interface ChatMessage {
  id: string;
  contactId: string;
  direction: "inbound" | "outbound" | "system";
  body: string;
  sentAt: string;
  deliveryState: DeliveryState;
}

export interface InviteDraft {
  id: string;
  email: string;
  status: "draft" | "sent" | "bundle_received" | "verified";
  opaqueToken: string;
}

export interface AppState {
  onboardingDone: boolean;
  profile: AccountProfile | null;
  contacts: Contact[];
  invites: InviteDraft[];
  messages: ChatMessage[];
  privacyMode: "balanced" | "padded" | "high";
  telemetryEnabled: boolean;
  statusNote: string;
}
