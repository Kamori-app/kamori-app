const RECOVERY_WORD_COUNT = 24;
const FILENAME_ENTROPY_BYTES = 8;

const normalizeWords = (phrase: string): string[] =>
  phrase.trim().split(/\s+/).filter(Boolean);

/**
 * Builds the plaintext recovery document downloaded by the browser.
 * The document is deliberately bilingual so it remains understandable if the
 * user later changes the application language.
 */
export function buildRecoveryKitText(username: string, phrase: string): string {
  const words = normalizeWords(phrase);
  if (words.length !== RECOVERY_WORD_COUNT) {
    throw new Error(`recovery kit must contain exactly ${RECOVERY_WORD_COUNT} words`);
  }
  const safeUsername = username.replace(/[\r\n]+/g, " ").trim();

  return [
    "KAMORI DATA RECOVERY KIT",
    "KEEP THIS FILE SECRET / ХРАНИТЕ ЭТОТ ФАЙЛ В СЕКРЕТЕ",
    "",
    `Account / Аккаунт: ${safeUsername}`,
    "",
    "Recovery words / Слова восстановления:",
    ...words.map((word, index) => `${index + 1}. ${word}`),
    "",
    "This plaintext file was created locally in your browser. Kamori never received or stored it.",
    "Этот незашифрованный файл создан локально в браузере. Kamori не получал и не хранил его.",
    "",
    "Never share this file. Keep it separately from your password and everyday device.",
    "Никому не передавайте файл. Храните его отдельно от пароля и основного устройства.",
    "",
    "Recovery / Восстановление: https://app.kamori.app/app/recovery",
    "Kamori support cannot recreate these words.",
    "Поддержка Kamori не может восстановить эти слова.",
    "",
  ].join("\n");
}

/** Returns a recognizable name without placing account data in the filename. */
export function buildRecoveryKitFilename(entropy: Uint8Array): string {
  if (entropy.length < FILENAME_ENTROPY_BYTES) {
    throw new Error(`filename entropy must contain at least ${FILENAME_ENTROPY_BYTES} bytes`);
  }
  const suffix = [...entropy.subarray(0, FILENAME_ENTROPY_BYTES)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  return `kamori-recovery-${suffix}.txt`;
}
