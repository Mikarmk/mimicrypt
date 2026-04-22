import { FormEvent, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { AppState, ChatMessage, Contact, DeliveryState } from "./model";
import {
  createContact,
  createInitialState,
  exportState,
  importState,
  isNativeShell,
  loadState,
  PROVIDER_PRESETS,
  resetState,
  saveState,
} from "./state";

interface NativeRuntimeInfo {
  appName: string;
  appVersion: string;
  appDataDir: string | null;
  stateStorePath: string | null;
  tauriEnv: string;
  packagingNote: string;
}

type ProviderKey = keyof typeof PROVIDER_PRESETS;

const deliveryLabels: Record<DeliveryState, string> = {
  queued: "Отправляется",
  smtp_accepted: "Отправлено",
  fetched_by_peer: "Доставлено",
  decrypted_by_peer: "Получено",
  read_by_peer: "Прочитано",
  failed: "Ошибка",
};

function trustLabel(trustState: Contact["trustState"]): string {
  if (trustState === "verified") return "Проверен";
  if (trustState === "blocked") return "Нужна проверка";
  return "Не проверен";
}

function messagePreview(messages: ChatMessage[], contactId: string): string {
  const latest = messages.find((message) => message.contactId === contactId);
  return latest?.body ?? "История пока пустая";
}

function chatSubtitle(contact: Contact): string {
  if (contact.trustState === "blocked") return "Ключ изменился. Чат заблокирован до подтверждения.";
  if (contact.trustState === "verified") return "На связи через проверенный ключ";
  return "Сначала сравните код безопасности";
}

export function App() {
  const [state, setState] = useState<AppState>(createInitialState());
  const [isHydrated, setIsHydrated] = useState(false);
  const [stateError, setStateError] = useState<string | null>(null);
  const [selectedProvider, setSelectedProvider] = useState<ProviderKey>("yandex");
  const [selectedContactId, setSelectedContactId] = useState<string | null>(state.contacts[0]?.id ?? null);
  const [search, setSearch] = useState("");
  const [contactEmail, setContactEmail] = useState("");
  const [contactNote, setContactNote] = useState("");
  const [draftMessage, setDraftMessage] = useState("");
  const [importBlob, setImportBlob] = useState("");
  const [nativeRuntime, setNativeRuntime] = useState<NativeRuntimeInfo | null>(null);
  const deferredMessage = useDeferredValue(draftMessage);
  const skipNextPersist = useRef(true);

  const selectedContact = state.contacts.find((contact) => contact.id === selectedContactId) ?? null;

  const filteredContacts = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return state.contacts;
    return state.contacts.filter((contact) => {
      return contact.email.toLowerCase().includes(needle) || contact.note.toLowerCase().includes(needle);
    });
  }, [search, state.contacts]);

  const messages = useMemo(
    () => state.messages.filter((message) => message.contactId === selectedContactId),
    [selectedContactId, state.messages],
  );

  useEffect(() => {
    let active = true;

    async function loadNativeRuntime() {
      if (!isNativeShell()) return;

      const { invoke } = await import("@tauri-apps/api/core");
      const runtime = await invoke<NativeRuntimeInfo>("runtime_info");
      if (active) setNativeRuntime(runtime);
    }

    loadNativeRuntime().catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;

    async function hydrateState() {
      try {
        const persisted = await loadState();
        if (!active) return;
        setState(persisted);
        setSelectedContactId(persisted.contacts[0]?.id ?? null);
      } catch {
        if (!active) return;
        setStateError("Не удалось загрузить локальный профиль. Создано новое устройство.");
      } finally {
        if (active) setIsHydrated(true);
      }
    }

    hydrateState().catch(() => {
      if (!active) return;
      setStateError("Не удалось загрузить локальный профиль. Создано новое устройство.");
      setIsHydrated(true);
    });

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!isHydrated) return;
    if (skipNextPersist.current) {
      skipNextPersist.current = false;
      return;
    }

    saveState(state).catch(() => {
      setStateError("Не удалось сохранить локальное состояние приложения.");
    });
  }, [isHydrated, state]);

  function completeOnboarding(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formData = new FormData(event.currentTarget);
    const preset = PROVIDER_PRESETS[selectedProvider];
    const displayName = String(formData.get("displayName") ?? "");
    const email = String(formData.get("email") ?? "");
    const smtpHost = String(formData.get("smtpHost") ?? preset.smtpHost);
    const imapHost = String(formData.get("imapHost") ?? preset.imapHost);

    setState((current) => ({
      ...current,
      onboardingDone: true,
      profile: {
        id: crypto.randomUUID(),
        displayName,
        email,
        mailboxMode: selectedProvider,
        smtpHost,
        imapHost,
        dedicatedMailboxRecommended: selectedProvider === "generic",
      },
      statusNote:
        "Контент шифруется до отправки. Доставка идёт через почту и может приходить не мгновенно.",
    }));
  }

  function commitContact() {
    if (!state.profile || !contactEmail.trim()) return;

    const contact = createContact(state.profile.email, contactEmail.trim(), contactNote.trim());
    const inviteId = crypto.randomUUID();

    setState((current) => ({
      ...current,
      contacts: [contact, ...current.contacts],
      invites: [
        {
          id: inviteId,
          email: contact.email,
          status: "sent",
          opaqueToken: `${inviteId.slice(0, 8)}-${contact.fingerprint}`,
        },
        ...current.invites,
      ],
      messages: [
        {
          id: crypto.randomUUID(),
          contactId: contact.id,
          direction: "system",
          body: "Инвайт отправлен. Когда собеседник ответит своим prekey bundle, откроется нормальный чат.",
          sentAt: new Date().toISOString(),
          deliveryState: "queued",
        },
        ...current.messages,
      ],
    }));

    setSelectedContactId(contact.id);
    setContactEmail("");
    setContactNote("");
  }

  function addContact(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    commitContact();
  }

  function verifySelectedContact() {
    if (!selectedContact) return;
    setState((current) => ({
      ...current,
      contacts: current.contacts.map((contact) =>
        contact.id === selectedContact.id ? { ...contact, trustState: "verified" } : contact,
      ),
      messages: [
        {
          id: crypto.randomUUID(),
          contactId: selectedContact.id,
          direction: "system",
          body: "Код безопасности подтверждён. Этот диалог теперь закреплён за проверенным identity key.",
          sentAt: new Date().toISOString(),
          deliveryState: "read_by_peer",
        },
        ...current.messages,
      ],
    }));
  }

  function simulateIdentityChange() {
    if (!selectedContact) return;
    setState((current) => ({
      ...current,
      contacts: current.contacts.map((contact) =>
        contact.id === selectedContact.id ? { ...contact, trustState: "blocked" } : contact,
      ),
      messages: [
        {
          id: crypto.randomUUID(),
          contactId: selectedContact.id,
          direction: "system",
          body: "Identity key контакта изменился. Отправка заблокирована, пока вы не перепроверите контакт.",
          sentAt: new Date().toISOString(),
          deliveryState: "failed",
        },
        ...current.messages,
      ],
    }));
  }

  function sendMessage(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedContact || !draftMessage.trim() || selectedContact.trustState === "blocked") return;

    const stages: DeliveryState[] = ["queued", "smtp_accepted", "fetched_by_peer"];

    setState((current) => ({
      ...current,
      messages: [
        {
          id: crypto.randomUUID(),
          contactId: selectedContact.id,
          direction: "outbound",
          body: draftMessage.trim(),
          sentAt: new Date().toISOString(),
          deliveryState: stages[Math.floor(Math.random() * stages.length)],
        },
        ...current.messages,
      ],
    }));

    setDraftMessage("");
  }

  async function resetAll() {
    const fresh = await resetState();
    skipNextPersist.current = true;
    setState(fresh);
    setSelectedContactId(null);
    setImportBlob("");
    setSelectedProvider("yandex");
    setStateError(null);
  }

  async function doImport() {
    if (!importBlob.trim()) return;
    try {
      const imported = importState(importBlob.trim());
      skipNextPersist.current = true;
      await saveState(imported);
      setState(imported);
      setSelectedContactId(imported.contacts[0]?.id ?? null);
      setStateError(null);
    } catch (error) {
      setStateError(error instanceof Error ? error.message : "Не удалось импортировать профиль.");
    }
  }

  async function copyExport() {
    try {
      await navigator.clipboard.writeText(exportState(state));
      setStateError(null);
    } catch {
      setStateError("Не удалось скопировать экспорт профиля в буфер обмена.");
    }
  }

  if (!isHydrated) {
    return (
      <div className="splash-shell">
        <div className="splash-card">
          <p className="eyebrow">Mimicrypt</p>
          <h1>Запускаем локальное устройство</h1>
          <p>Открываем профиль, загружаем ключи и поднимаем рабочий чат-клиент.</p>
        </div>
      </div>
    );
  }

  if (!state.onboardingDone) {
    const preset = PROVIDER_PRESETS[selectedProvider];
    return (
      <div className="welcome-shell">
        <section className="welcome-preview">
          <div className="preview-header">
            <p className="brand-mark">Mimicrypt Desktop</p>
            <div className="preview-badge">Telegram-like layout</div>
          </div>
          <div className="preview-copy">
            <h1>Нормальный мессенджер, а не крипто-пульт.</h1>
            <p>
              После регистрации у тебя слева будут диалоги и настройки, справа обычный чат. Почта остаётся только
              транспортом, а не интерфейсом.
            </p>
          </div>
          <div className="preview-window">
            <div className="preview-sidebar">
              <div className="preview-profile">
                <div className="avatar-circle">M</div>
                <div>
                  <strong>Марат</strong>
                  <small>{preset.label}</small>
                </div>
              </div>
              <div className="preview-search">Поиск</div>
              <div className="preview-dialog active">
                <strong>alice@yandex.ru</strong>
                <span>Код подтверждён</span>
              </div>
              <div className="preview-dialog">
                <strong>team@mail.ru</strong>
                <span>Ожидает проверки</span>
              </div>
            </div>
            <div className="preview-chat">
              <div className="preview-chat-head">
                <strong>alice@yandex.ru</strong>
                <small>Проверенный контакт</small>
              </div>
              <div className="bubble inbound">Сверили safety number, можно работать.</div>
              <div className="bubble outbound">Отлично. Дальше всё выглядит как обычный чат.</div>
            </div>
          </div>
        </section>

        <section className="welcome-card">
          <div className="welcome-card-head">
            <p className="eyebrow">Регистрация</p>
            <h2>Подключи почтовый аккаунт</h2>
            <p>Начни с Yandex или Mail.ru. Потом сможешь использовать любой IMAP/SMTP провайдер.</p>
          </div>

          <div className="provider-grid">
            {(Object.keys(PROVIDER_PRESETS) as ProviderKey[]).map((providerKey) => {
              const provider = PROVIDER_PRESETS[providerKey];
              return (
                <button
                  key={providerKey}
                  type="button"
                  className={selectedProvider === providerKey ? "provider-card active" : "provider-card"}
                  onClick={() => setSelectedProvider(providerKey)}
                >
                  <strong>{provider.label}</strong>
                  <small>{provider.domainHint}</small>
                </button>
              );
            })}
          </div>

          <form className="welcome-form" onSubmit={completeOnboarding}>
            <label>
              Имя в приложении
              <input name="displayName" placeholder="Марат" required />
            </label>
            <label>
              Почтовый адрес
              <input name="email" type="email" placeholder={`you${preset.domainHint}`} required />
            </label>
            <div className="welcome-grid">
              <label>
                IMAP
                <input name="imapHost" defaultValue={preset.imapHost} required />
              </label>
              <label>
                SMTP
                <input name="smtpHost" defaultValue={preset.smtpHost} required />
              </label>
            </div>
            <button type="submit" className="primary-cta">
              Открыть приложение
            </button>
            <p className="tiny-note">
              Реальная доставка идёт через email. Контент шифруется на устройстве до SMTP-отправки.
            </p>
          </form>
        </section>
      </div>
    );
  }

  return (
    <div className="desktop-shell">
      <aside className="sidebar">
        <header className="sidebar-header">
          <div className="profile-chip">
            <div className="avatar-circle">{state.profile?.displayName?.slice(0, 1).toUpperCase()}</div>
            <div>
              <strong>{state.profile?.displayName}</strong>
              <small>{state.profile?.email}</small>
            </div>
          </div>
          <div className="sidebar-header-meta">
            <span>{PROVIDER_PRESETS[state.profile?.mailboxMode ?? "generic"].label}</span>
            <span>Desktop</span>
          </div>
        </header>

        <div className="sidebar-search-wrap">
          <input
            className="search-input"
            placeholder="Поиск по контактам"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </div>

        <form className="quick-add" onSubmit={addContact}>
          <div className="quick-add-head">
            <strong>Новый контакт</strong>
            <span>Инвайт уйдёт через email</span>
          </div>
          <input
            value={contactEmail}
            onChange={(event) => setContactEmail(event.target.value)}
            placeholder="alice@yandex.ru"
          />
          <input value={contactNote} onChange={(event) => setContactNote(event.target.value)} placeholder="Имя или заметка" />
          <button type="submit">Добавить контакт</button>
        </form>

        <section className="dialog-list">
          {filteredContacts.length === 0 ? (
            <div className="empty-sidebar">Добавь первый контакт. Он сразу появится здесь как отдельный диалог.</div>
          ) : (
            filteredContacts.map((contact) => (
              <button
                key={contact.id}
                type="button"
                className={selectedContactId === contact.id ? "dialog-row active" : "dialog-row"}
                onClick={() => setSelectedContactId(contact.id)}
              >
                <div className="avatar-circle small">{(contact.note || contact.email).slice(0, 1).toUpperCase()}</div>
                <div className="dialog-meta">
                  <div className="dialog-meta-top">
                    <strong>{contact.note || contact.email}</strong>
                    <span className={`status-pill ${contact.trustState}`}>{trustLabel(contact.trustState)}</span>
                  </div>
                  <span className="dialog-email">{contact.email}</span>
                  <p>{messagePreview(state.messages, contact.id)}</p>
                </div>
              </button>
            ))
          )}
        </section>

        <section className="sidebar-settings">
          <div className="settings-card">
            <div className="settings-head">
              <strong>Настройки</strong>
              <span>Локальный профиль</span>
            </div>
            <div className="settings-actions">
              <button type="button" className="ghost-button" onClick={copyExport}>
                Экспорт
              </button>
              <button type="button" className="ghost-button" onClick={resetAll}>
                Сброс
              </button>
            </div>
            <textarea
              rows={4}
              placeholder="Вставь сюда экспортированный профиль"
              value={importBlob}
              onChange={(event) => setImportBlob(event.target.value)}
            />
            <button type="button" className="ghost-button wide" onClick={doImport}>
              Импортировать профиль
            </button>
          </div>

          <div className="settings-card subtle">
            <strong>Статус</strong>
            <p>{state.statusNote}</p>
            {stateError ? <p className="error-note">{stateError}</p> : null}
            {nativeRuntime ? (
              <small>
                {nativeRuntime.appName} {nativeRuntime.appVersion} · {nativeRuntime.tauriEnv}
              </small>
            ) : null}
          </div>
        </section>
      </aside>

      <main className="conversation">
        {selectedContact ? (
          <>
            <header className="conversation-head">
              <div className="chat-stage-user">
                <div className="avatar-circle">{(selectedContact.note || selectedContact.email).slice(0, 1).toUpperCase()}</div>
                <div>
                  <strong>{selectedContact.note || selectedContact.email}</strong>
                  <small>{chatSubtitle(selectedContact)}</small>
                </div>
              </div>

              <div className="conversation-actions">
                <span className={`header-trust ${selectedContact.trustState}`}>{trustLabel(selectedContact.trustState)}</span>
                <button type="button" className="ghost-button" onClick={verifySelectedContact}>
                  Проверить код
                </button>
                <button type="button" className="ghost-button danger-text" onClick={simulateIdentityChange}>
                  Смена ключа
                </button>
              </div>
            </header>

            <div className={`security-banner ${selectedContact.trustState}`}>
              <div>
                <strong>Safety number</strong>
                <p>{selectedContact.safetyNumber}</p>
              </div>
              <span>{selectedContact.fingerprint}</span>
            </div>

            <section className="chat-stream">
              {messages.length === 0 ? (
                <div className="empty-chat">Отправь первое сообщение. История будет выглядеть как обычный десктопный чат.</div>
              ) : (
                messages
                  .slice()
                  .reverse()
                  .map((message) => (
                    <article key={message.id} className={`chat-bubble ${message.direction}`}>
                      <p>{message.body}</p>
                      <footer>
                        <span>{new Date(message.sentAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
                        <span>{deliveryLabels[message.deliveryState]}</span>
                      </footer>
                    </article>
                  ))
              )}
            </section>

            <form className="composer-bar" onSubmit={sendMessage}>
              <textarea
                value={draftMessage}
                onChange={(event) => setDraftMessage(event.target.value)}
                placeholder={
                  selectedContact.trustState === "blocked"
                    ? "Сначала перепроверь identity key контакта"
                    : "Напишите сообщение"
                }
                disabled={selectedContact.trustState === "blocked"}
              />
              <div className="composer-tools">
                <span className="composer-hint">
                  {selectedContact.trustState === "blocked"
                    ? "Отправка остановлена до повторной верификации"
                    : `После упаковки и padding: ${deferredMessage.length} символов в черновике`}
                </span>
                <button type="submit" disabled={selectedContact.trustState === "blocked"}>
                  Отправить
                </button>
              </div>
            </form>
          </>
        ) : (
          <div className="chat-placeholder">
            <h2>Выберите диалог слева</h2>
            <p>Список контактов и настройки остаются в sidebar, а справа открывается вся переписка.</p>
          </div>
        )}
      </main>
    </div>
  );
}
