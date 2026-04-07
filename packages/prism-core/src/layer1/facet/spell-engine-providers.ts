/**
 * Dictionary providers — factories for creating DictionaryProviders
 * that load Hunspell .aff/.dic files from various sources.
 */

import type { DictionaryProvider, DictionaryData } from './spell-engine-types';

// ── URL-based provider (browser/CDN) ─────────────────────────────────────────

interface UrlProviderConfig {
  id: string;
  label: string;
  language: string;
  /** URL to the .aff file. */
  affUrl: string;
  /** URL to the .dic file. */
  dicUrl: string;
  /** Optional fetch options (headers, credentials, etc.). */
  fetchOptions?: RequestInit;
}

/**
 * Create a DictionaryProvider that fetches .aff/.dic from URLs.
 * Caches the result after first load.
 */
export function createUrlDictionaryProvider(
  config: UrlProviderConfig,
): DictionaryProvider {
  let cached: DictionaryData | undefined;

  return {
    id: config.id,
    label: config.label,
    language: config.language,
    async load(): Promise<DictionaryData> {
      if (cached) return cached;
      const [affRes, dicRes] = await Promise.all([
        fetch(config.affUrl, config.fetchOptions),
        fetch(config.dicUrl, config.fetchOptions),
      ]);
      if (!affRes.ok)
        throw new Error(
          `Failed to fetch .aff: ${affRes.status} ${affRes.statusText}`,
        );
      if (!dicRes.ok)
        throw new Error(
          `Failed to fetch .dic: ${dicRes.status} ${dicRes.statusText}`,
        );
      cached = {
        aff: await affRes.text(),
        dic: await dicRes.text(),
      };
      return cached;
    },
  };
}

// ── Static data provider (bundled/pre-loaded) ────────────────────────────────

interface StaticProviderConfig {
  id: string;
  label: string;
  language: string;
  /** Pre-loaded .aff content. */
  aff: string;
  /** Pre-loaded .dic content. */
  dic: string;
}

/** Create a DictionaryProvider from pre-loaded dictionary data. */
export function createStaticDictionaryProvider(
  config: StaticProviderConfig,
): DictionaryProvider {
  const data: DictionaryData = { aff: config.aff, dic: config.dic };
  return {
    id: config.id,
    label: config.label,
    language: config.language,
    async load(): Promise<DictionaryData> {
      return data;
    },
  };
}

// ── Lazy-load provider (callback) ────────────────────────────────────────────

interface LazyProviderConfig {
  id: string;
  label: string;
  language: string;
  /** Callback to load dictionary data. Called once, result cached. */
  loader: () => Promise<DictionaryData>;
}

/** Create a DictionaryProvider that lazily loads via a callback. */
export function createLazyDictionaryProvider(
  config: LazyProviderConfig,
): DictionaryProvider {
  let cached: DictionaryData | undefined;

  return {
    id: config.id,
    label: config.label,
    language: config.language,
    async load(): Promise<DictionaryData> {
      if (cached) return cached;
      cached = await config.loader();
      return cached;
    },
  };
}
