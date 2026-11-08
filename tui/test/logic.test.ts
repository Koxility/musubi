import { describe, expect, test } from 'bun:test';
import { Form, field } from '../src/form';
import { Keymap, toKeyEvent, type InkKey, type KeyEvent } from '../src/keys';
import type { Api } from '../src/rpc';
import { Store, mappingFromForm } from '../src/store';
import { countdown, formatTime, fuzzyFilter, visibleWindow } from '../src/text';
import type { AuthStatus, LibraryRow, ListRow, Mapping, Session } from '../src/types';

const NO_KEY: InkKey = {
  upArrow: false, downArrow: false, leftArrow: false, rightArrow: false, pageDown: false, pageUp: false,
  home: false, end: false, return: false, escape: false, ctrl: false, shift: false, tab: false,
  backspace: false, delete: false, meta: false,
};
const key = (name: string, text = ''): KeyEvent => ({ name, text });
const char = (c: string): KeyEvent => ({ name: c, text: c });
const auth = (jellyfin: boolean, anilist: boolean): AuthStatus => ({
  jellyfin_user: jellyfin ? 'user' : null,
  jellyfin_url: jellyfin ? 'http://server:30013' : null,
  anilist_user: anilist ? 'ani' : null,
  anilist_logged_in: anilist,
});
const session = (a: AuthStatus, setup = true): Session => ({
  auth: a, viewer: null, setup_needed: setup, jellyfin_url: null, client_id: '56388',
  redirect: 'http://127.0.0.1:47823/callback', auto_sync: true, score_hint: '0-100', keys: {},
});

class FakeApi implements Api {
  calls: [string, any][] = [];
  current = session(auth(false, false));
  failures = new Map<string, string>();
  async call<T>(method: string, params: unknown = {}): Promise<T> {
    this.calls.push([method, params]);
    const failure = this.failures.get(method);
    if (failure) throw new Error(failure);
    switch (method) {
      case 'session': case 'refresh_lists': return this.current as T;
      case 'list': case 'cached_library': case 'library': case 'sync_diff': return [] as T;
      case 'overview': return { airing: [], anime_in_progress: [], manga_in_progress: [] } as T;
      case 'home': return { continue_watching: [], next_up: [], latest: [] } as T;
      case 'library_choices': return { filters: { genres: [], years: [] }, libraries: [] } as T;
      case 'mapping_data': return { reviews: [], mappings: [] } as T;
      case 'sync_status': return { pending: [], log: [] } as T;
      case 'login_jellyfin': this.current = session(auth(true, this.current.auth.anilist_logged_in)); return 'user' as T;
      case 'login_anilist_browser': this.current = session(auth(this.current.auth.jellyfin_user !== null, true)); return 'ani' as T;
      default: return undefined as T;
    }
  }
  named(method: string) { return this.calls.filter(([m]) => m === method); }
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const purpose = (store: Store) => (store.modal?.type === 'form' ? store.modal.purpose.type : null);
const form = (store: Store) => {
  if (store.modal?.type !== 'form') throw new Error('form expected');
  return store.modal.form;
};

async function started(a: AuthStatus, setup = true) {
  const api = new FakeApi();
  api.current = session(a, setup);
  const store = new Store(api);
  await store.start();
  await settle();
  return { api, store };
}

describe('text helpers', () => {
  test('countdown picks the two largest units', () => {
    expect(countdown(0)).toBe('airing');
    expect(countdown(30)).toBe('1m');
    expect(countdown(3720)).toBe('1h 2m');
    expect(countdown(2 * 86400 + 3 * 3600 + 59)).toBe('2d 3h');
  });
  test('formatTime', () => {
    expect(formatTime(65)).toBe('1:05');
    expect(formatTime(3661)).toBe('1:01:01');
  });
  test('window keeps the selection visible', () => {
    expect(visibleWindow(0, 3, 10)).toEqual([0, 3]);
    expect(visibleWindow(0, 100, 10)).toEqual([0, 10]);
    expect(visibleWindow(50, 100, 10)).toEqual([45, 55]);
    expect(visibleWindow(99, 100, 10)).toEqual([90, 100]);
  });
  test('fuzzy filter ranks prefix matches first and keeps all for empty', () => {
    expect(fuzzyFilter(['b', 'a'], '')).toEqual([0, 1]);
    expect(fuzzyFilter(['Mob Psycho', 'Frieren', 'xfxrx'], 'fr')).toEqual([1, 2]);
  });
});

describe('keys', () => {
  test('maps ink keys to canonical names', () => {
    expect(toKeyEvent('', { ...NO_KEY, return: true }).name).toBe('enter');
    expect(toKeyEvent('', { ...NO_KEY, tab: true, shift: true }).name).toBe('backtab');
    expect(toKeyEvent('d', { ...NO_KEY, ctrl: true }).name).toBe('ctrl-d');
    expect(toKeyEvent(' ', NO_KEY)).toEqual({ name: 'space', text: ' ' });
    expect(toKeyEvent('G', NO_KEY)).toEqual({ name: 'G', text: 'G' });
    expect(toKeyEvent('pasted text', NO_KEY)).toEqual({ name: '', text: 'pasted text' });
    expect(toKeyEvent('', { ...NO_KEY, delete: true }).name).toBe('backspace');
  });
  test('overrides replace defaults but arrows keep working', () => {
    const map = new Keymap({ down: 'J', play: 'P' });
    expect(map.action('J')).toBe('down');
    expect(map.action('down')).toBe('down');
    expect(map.action('P')).toBe('play');
    expect(map.label('play')).toBe('P');
  });
});

describe('form', () => {
  test('edits, navigates, masks and cycles', () => {
    const f = new Form('t', [
      field.text('name', 'Name', 'ab'),
      field.secret('pw', 'Password'),
      field.choice('c', 'Choice', ['x', 'y'], 'x'),
      field.toggle('t', 'Toggle', false),
    ]);
    f.onKey(char('c'));
    f.onKey(key('backspace'));
    f.onKey(char('d'));
    expect(f.value('name')).toBe('abd');
    f.onKey(key('tab'));
    f.onKey(key('', 'secret'));
    expect(f.value('pw')).toBe('secret');
    f.onKey(key('tab'));
    f.onKey(key('right'));
    expect(f.value('c')).toBe('y');
    f.onKey(key('tab'));
    f.onKey(key('space', ' '));
    expect(f.isOn('t')).toBe(true);
    expect(f.onKey(key('enter'))).toBe('submit');
    expect(f.onKey(key('esc'))).toBe('cancel');
  });
});

describe('setup flow', () => {
  test('starts at the first missing login with the client id prefilled', async () => {
    const { store } = await started(auth(true, false));
    expect(store.setup).toBe('anilist');
    expect(form(store).value('client_id')).toBe('56388');
  });

  test('esc walks through and finishes without changing sync', async () => {
    const { store, api } = await started(auth(false, false));
    expect(purpose(store)).toBe('login_jellyfin');
    store.onKey(key('esc'));
    expect(purpose(store)).toBe('login_anilist');
    store.onKey(key('esc'));
    expect(purpose(store)).toBe('setup_sync');
    store.onKey(key('esc'));
    await settle();
    expect(store.setup).toBeNull();
    expect(store.modal).toBeNull();
    expect(api.named('finish_setup')).toEqual([['finish_setup', { auto_sync: null }]]);
  });

  test('successful logins advance and enter saves auto sync', async () => {
    const { store, api } = await started(auth(false, false));
    const f = form(store);
    f.fields[0]!.value = 'server:30013';
    f.fields[1]!.value = 'user';
    store.onKey(key('enter'));
    await settle(); await settle();
    expect(purpose(store)).toBe('login_anilist');
    store.onKey(key('enter'));
    await settle(); await settle();
    expect(purpose(store)).toBe('setup_sync');
    store.onKey(key('space', ' '));
    store.onKey(key('enter'));
    await settle();
    expect(api.named('finish_setup')).toEqual([['finish_setup', { auto_sync: false }]]);
    expect(store.setup).toBeNull();
  });

  test('a failed login stays in the form and shows the error', async () => {
    const { store, api } = await started(auth(true, false));
    api.failures.set('login_anilist_browser', 'port in use');
    store.onKey(key('enter'));
    await settle(); await settle();
    expect(purpose(store)).toBe('login_anilist');
    expect(form(store).error).toBe('port in use');
  });

  test('no setup and no jellyfin opens the login form', async () => {
    const { store } = await started(auth(false, false), false);
    expect(store.setup).toBeNull();
    expect(purpose(store)).toBe('login_jellyfin');
  });
});

describe('navigation and actions', () => {
  function entry(id: number, progress: number, total = 12): ListRow {
    return {
      id, status: 'CURRENT', score_raw: 0, progress, progress_volumes: 0, repeat: 0, priority: 0,
      private: false, hidden_from_status_lists: false, notes: '', custom_lists: [], advanced_scores: [],
      started_at: { year: null, month: null, day: null }, completed_at: { year: null, month: null, day: null },
      updated_at: 0, score_text: '', behind: 0,
      media: {
        id, kind: 'ANIME', title: { romaji: `T${id}`, english: null, native: null }, format: 'TV', status: null,
        episodes: total, chapters: null, volumes: null, season_year: 2024, description: null, genres: [],
        average_score: null, next_airing: null,
      },
    };
  }

  test('number keys switch tabs and q quits', async () => {
    const { store } = await started(auth(true, true), false);
    store.onKey(char('3'));
    expect(store.tab).toBe('anilist');
    store.onKey(char('5'));
    expect(store.tab).toBe('sync');
    store.onKey(char('q'));
    expect(store.shouldQuit).toBe(true);
  });

  test('t cycles overview, anime and manga', async () => {
    const { store } = await started(auth(true, true), false);
    store.onKey(char('3'));
    expect(store.anilistView).toBe('overview');
    store.onKey(char('t'));
    expect(store.anilistView).toBe('ANIME');
    store.onKey(char('t'));
    expect(store.anilistView).toBe('MANGA');
    store.onKey(char('t'));
    expect(store.anilistView).toBe('overview');
  });

  test('plus raises progress but not past the total', async () => {
    const { store, api } = await started(auth(true, true), false);
    store.onKey(char('3'));
    store.overview.airing = [entry(1, 12)];
    store.onKey(char('+'));
    store.overview.airing = [entry(1, 3)];
    store.onKey(char('+'));
    await settle();
    expect(api.named('save_entry').map(([, p]) => p.progress)).toEqual([12, 4]);
  });

  test('home enter plays episodes and opens series', async () => {
    const { store, api } = await started(auth(true, true), false);
    const base = { title: 'S', subtitle: null, overview: null, year: null, runtime_secs: null, position_secs: 0 };
    store.shelves = {
      continue_watching: [{ ...base, id: 'e1', library_id: 's1', is_episode: true }],
      next_up: [],
      latest: [],
    };
    store.onKey(key('enter'));
    await settle();
    expect(api.named('play')).toEqual([['play', { library_id: 's1', episode_id: 'e1' }]]);
  });

  test('commands report usage errors and edit the selection', async () => {
    const { store, api } = await started(auth(true, true), false);
    store.onKey(char('3'));
    store.overview.airing = [entry(1, 3)];
    store.runCommand('score 85');
    store.runCommand('status done');
    await settle();
    expect(api.named('save_entry').map(([, p]) => p)).toEqual([
      { media_id: 1, score: '85' },
      { media_id: 1, status: 'COMPLETED' },
    ]);
    store.runCommand('nope');
    expect(store.notice?.text).toBe('unknown command `nope`');
  });

  test('entry form only sends changed fields', async () => {
    const { store, api } = await started(auth(true, true), false);
    store.onKey(char('3'));
    store.overview.airing = [entry(1, 3)];
    store.onKey(char('e'));
    const f = form(store);
    f.fields.find((x) => x.key === 'progress')!.value = '5';
    store.onKey(key('enter'));
    await settle();
    expect(api.named('save_entry').map(([, p]) => p)).toEqual([{ media_id: 1, progress: 5 }]);
    expect(store.modal).toBeNull();
  });

  test('library filter narrows the list and esc clears it', async () => {
    const { store } = await started(auth(true, true), false);
    const row = (id: string, name: string): LibraryRow => ({
      item: { id, kind: 'Series', name, overview: null, year: null, status: null, genres: [], community_rating: null, runtime_secs: null, played: false, unplayed_count: null },
      mappings: [], entry: null, needs_review: false,
    });
    store.rows = [row('a', 'Frieren'), row('b', 'Mob Psycho')];
    store.onKey(char('2'));
    store.onKey(char('/'));
    store.onKey(char('m'));
    store.onKey(char('o'));
    expect(store.visibleLibrary().map((r) => r.item.id)).toEqual(['b']);
    store.onKey(key('esc'));
    expect(store.visibleLibrary()).toHaveLength(2);
  });

  test('ctrl-c always quits, even in a form', async () => {
    const { store } = await started(auth(false, false));
    store.onKey(key('ctrl-c'));
    expect(store.shouldQuit).toBe(true);
  });
});

describe('mapping form', () => {
  const old: Mapping = {
    jellyfin_id: 's', season: 1, ep_start: 1, ep_end: 12, media_id: 9, offset: 0,
    source: 'Title', confidence: 0.8, title: 'Show',
  };
  const build = (end: string, start = '1') => new Form('m', [
    field.text('media', '', '9'), field.text('season', '', '2'), field.text('start', '', start),
    field.text('end', '', end), field.text('offset', '', '-3'),
  ]);
  test('empty end is open and marks manual', () => {
    const built = mappingFromForm(build(''), old);
    expect(built.ep_end).toBeNull();
    expect(built.source).toBe('Manual');
    expect(built.offset).toBe(-3);
  });
  test('rejects end before start', () => {
    expect(() => mappingFromForm(build('2', '5'), old)).toThrow();
  });
});
