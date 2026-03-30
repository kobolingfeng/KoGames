import en from './en';
import zh from './zh';

const locales: Record<string, Record<string, string>> = { en, zh };

function detectLocale(): string {
  const lang = navigator.language.toLowerCase();
  if (lang.startsWith('zh')) return 'zh';
  return 'en';
}

let currentLocale = detectLocale();

export function t(key: string): string {
  return locales[currentLocale]?.[key] ?? locales['en']?.[key] ?? key;
}

export function setLocale(locale: string) {
  if (locales[locale]) currentLocale = locale;
}

export function getLocale(): string {
  return currentLocale;
}
