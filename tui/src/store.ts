// Zustand und Bedienlogik der TUI. Enthält kein Rendering und ist dadurch direkt testbar.

import { Form, field } from './form';
import { Keymap, type Action, type KeyEvent } from './keys';
import type { Api } from './rpc';
import { fuzzyFilter } from './text';
import {
  STATUSES, formatDate, parseStatus, preferredTitle, statusLabel, totalUnits,
  type AuthStatus, type CoreEvent, type EpisodeRow, type HomeShelves, type ItemKind, type Library,
  type LibraryFilters, type LibraryItem, type LibraryQuery, type LibraryRow, type LibrarySort,
  type ListEntry, type ListRow, type ListStatus, type Mapping, type Media, type MediaKind,
  type OutboxItem, type Overview, type PlaybackState, type PlayerCommand, type ReviewItem,
  type Session, type ShelfItem, type SyncDirection, type SyncLogEntry, type SyncReport, type SyncRow,
} from './types';

const SEEK_STEP = 10;
const PAGE = 15;
const NOTICE_SECONDS = 6;

export const TABS = ['home', 'library', 'anilist', 'mapping', 'sync'] as const;
export type Tab = (typeof TABS)[number];
export type SetupStep = 'jellyfin' | 'anilist' | 'sync';
export type AniListView = 'overview' | MediaKind;
export type MappingView = 'review' | 'mapped';

type Purpose =
  | { type: 'login_jellyfin' }
  | { type: 'login_anilist' }
  | { type: 'setup_sync' }
  | { type: 'edit_entry'; entry: ListEntry }
  | { type: 'status'; entry: ListEntry }
  | { type: 'score'; entry: ListEntry }
  | { type: 'filters' }
  | { type: 'map_manual'; item: LibraryItem }
  | { type: 'edit_mapping'; mapping: Mapping }
  | { type: 'search_add' }
  | { type: 'add_status'; media: Media };

export type Modal =
  | { type: 'help' }
  | { type: 'form'; form: Form; purpose: Purpose }
  | { type: 'confirm'; text: string; run: () => void }
  | { type: 'results'; results: Media[]; selected: number };

export interface SeriesView {
  item: LibraryItem;
  episodes: EpisodeRow[];
  selected: number;
}

export interface HomeRow {
  section: string;
  item: ShelfItem;
}

export interface OverviewRow {
  section: string;
  entry: ListRow;
}

export interface Notice {
  ok: boolean;
  text: string;
  at: number;
}

const EMPTY_AUTH: AuthStatus = {
  jellyfin_user: null,
  jellyfin_url: null,
  anilist_user: null,
  anilist_logged_in: false,
};

const DEFAULT_QUERY: LibraryQuery = {
  library_id: null, search: null, genres: [], years: [], status: null, kind: null,
  unplayed_only: false, sort: 'Name', start: 0, limit: null,
};

const SORT_LABELS: [LibrarySort, string][] = [
  ['Name', 'Name'], ['Year', 'Year'], ['DateAdded', 'Date added'], ['Rating', 'Rating'],
];

function clamp(value: number, count: number): number {
  return Math.min(value, Math.max(count - 1, 0));
}

function splitList(text: string): string[] {
  return text.split(',').map((s) => s.trim()).filter((s) => s !== '');
}

function advancedValue(entry: ListEntry, name: string): number {
  return entry.advanced_scores.find(([n]) => n === name)?.[1] ?? 0;
}

function parseAdvanced(text: string, names: string[], entry: ListEntry): number[] {
  const given = new Map<string, number>();
  for (const part of splitList(text)) {
    const [name, value] = part.split('=').map((s) => s.trim());
    if (!name || value === undefined) throw new Error('use name=value, name=value');
    if (!names.includes(name)) throw new Error(`unknown category ${name}`);
    const number = Number(value);
    if (value === '' || Number.isNaN(number)) throw new Error(`\`${value}\` is not a number`);
    given.set(name, number);
  }
  return names.map((name) => given.get(name) ?? advancedValue(entry, name));
}

function wholeNumber(text: string, name: string): number {
  if (!/^\d+$/.test(text.trim())) throw new Error(`${name} must be a whole number`);
  return Number(text.trim());
}

export function mappingFromForm(form: Form, old: Mapping): Mapping {
  const last = form.value('end').trim();
  const first = wholeNumber(form.value('start'), 'first episode');
  const end = last === '' ? null : wholeNumber(last, 'last episode');
  if (end !== null && end < first) throw new Error('the last episode must not be before the first');
  const offset = form.value('offset').trim();
  if (!/^-?\d+$/.test(offset)) throw new Error('offset must be a whole number');
  return {
    ...old,
    media_id: wholeNumber(form.value('media'), 'AniList ID'),
    season: wholeNumber(form.value('season'), 'season'),
    ep_start: first,
    ep_end: end,
    offset: Number(offset),
    source: 'Manual',
    confidence: 1,
  };
}

export class Store {
  version = 0;
  tab: Tab = 'home';
  shouldQuit = false;
  session: Session | null = null;
  auth: AuthStatus = EMPTY_AUTH;
  keymap = new Keymap();
  modal: Modal | null = null;
  setup: SetupStep | null = null;
  notice: Notice | null = null;
  playback: PlaybackState | null = null;
  matchProgress: [number, number] | null = null;
  rateLimited: number | null = null;
  now = Math.floor(Date.now() / 1000);

  mode: 'normal' | 'filter' | 'command' = 'normal';
  input = '';

  shelves: HomeShelves = { continue_watching: [], next_up: [], latest: [] };
  homeSelected = 0;

  rows: LibraryRow[] = [];
  librarySelected = 0;
  libraryFilter = '';
  query: LibraryQuery = { ...DEFAULT_QUERY };
  filters: LibraryFilters = { genres: [], years: [] };
  libraries: Library[] = [];
  series: SeriesView | null = null;
  private libraryGeneration = 0;

  anime: ListRow[] = [];
  manga: ListRow[] = [];
  overview: Overview = { airing: [], anime_in_progress: [], manga_in_progress: [] };
  anilistView: AniListView = 'overview';
  statusFilter: ListStatus | null = null;
  listSelected = 0;
  listFilter = '';

  reviews: ReviewItem[] = [];
  mappings: Mapping[] = [];
  mappingView: MappingView = 'review';
  mappingSelected = 0;

  diff: SyncRow[] = [];
  diffSelected = 0;
  pending: OutboxItem[] = [];
  log: SyncLogEntry[] = [];

  private readonly listeners = new Set<() => void>();

  constructor(private readonly api: Api) {}

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getVersion = (): number => this.version;

  emit(): void {
    this.version += 1;
    for (const listener of this.listeners) listener();
  }

  // Abgeleitete Listen ---------------------------------------------------------------

  homeRows(): HomeRow[] {
    const section = (name: string, items: ShelfItem[]) => items.map((item) => ({ section: name, item }));
    return [
      ...section('Continue Watching', this.shelves.continue_watching),
      ...section('Next Up', this.shelves.next_up),
      ...section('Recently Added', this.shelves.latest),
    ];
  }

  visibleLibrary(): LibraryRow[] {
    return fuzzyFilter(this.rows.map((r) => r.item.name), this.libraryFilter).map((i) => this.rows[i]!);
  }

  overviewRows(): OverviewRow[] {
    const section = (name: string, entries: ListRow[]) => entries.map((entry) => ({ section: name, entry }));
    return [
      ...section('Airing', this.overview.airing),
      ...section('Anime in Progress', this.overview.anime_in_progress),
      ...section('Manga in Progress', this.overview.manga_in_progress),
    ];
  }

  listKind(): MediaKind {
    return this.anilistView === 'MANGA' ? 'MANGA' : 'ANIME';
  }

  visibleList(): ListRow[] {
    if (this.anilistView === 'overview') return this.overviewRows().map((r) => r.entry);
    const source = this.anilistView === 'ANIME' ? this.anime : this.manga;
    const filtered = source.filter((e) => this.statusFilter === null || e.status === this.statusFilter);
    return fuzzyFilter(filtered.map((e) => preferredTitle(e.media.title)), this.listFilter).map((i) => filtered[i]!);
  }

  mappingRows(): Mapping[] {
    return this.mappings;
  }

  private mappingCount(): number {
    return this.mappingView === 'review' ? this.reviews.length : this.mappings.length;
  }

  // Asynchrone Arbeit ----------------------------------------------------------------

  private report(error: unknown): void {
    const text = error instanceof Error ? error.message : String(error);
    if (this.modal?.type === 'form') this.modal.form.error = text;
    this.setNotice(false, text);
    this.emit();
  }

  private run(work: Promise<unknown>): void {
    work.then(() => this.emit(), (error: unknown) => this.report(error));
  }

  setNotice(ok: boolean, text: string): void {
    this.notice = { ok, text, at: Date.now() };
  }

  private async loadSession(): Promise<void> {
    this.session = await this.api.call<Session>('session');
    this.auth = this.session.auth;
    this.keymap = new Keymap(this.session.keys);
  }

  private async loadHome(): Promise<void> {
    if (this.auth.jellyfin_user === null) return;
    this.shelves = await this.api.call<HomeShelves>('home');
    this.homeSelected = clamp(this.homeSelected, this.homeRows().length);
  }

  private async loadLibrary(): Promise<void> {
    if (this.auth.jellyfin_user === null) return;
    this.libraryGeneration += 1;
    const generation = this.libraryGeneration;
    const rows = await this.api.call<LibraryRow[]>('library', this.query);
    if (generation !== this.libraryGeneration) return;
    this.rows = rows;
    this.librarySelected = clamp(this.librarySelected, this.visibleLibrary().length);
  }

  private async loadChoices(): Promise<void> {
    if (this.auth.jellyfin_user === null) return;
    const choices = await this.api.call<{ filters: LibraryFilters; libraries: Library[] }>('library_choices');
    this.filters = choices.filters;
    this.libraries = choices.libraries;
  }

  private async loadEpisodes(id: string): Promise<void> {
    const episodes = await this.api.call<EpisodeRow[]>('episodes', { id });
    if (this.series?.item.id !== id) return;
    const firstOpen = episodes.findIndex((e) => !e.episode.played);
    const fresh = this.series.episodes.length === 0;
    this.series.episodes = episodes;
    this.series.selected = fresh ? Math.max(firstOpen, 0) : clamp(this.series.selected, episodes.length);
  }

  private async loadLists(): Promise<void> {
    const [anime, manga, overview] = await Promise.all([
      this.api.call<ListRow[]>('list', { kind: 'ANIME' }),
      this.api.call<ListRow[]>('list', { kind: 'MANGA' }),
      this.api.call<Overview>('overview'),
    ]);
    this.anime = anime;
    this.manga = manga;
    this.overview = overview;
    this.listSelected = clamp(this.listSelected, this.visibleList().length);
  }

  private async refreshLists(): Promise<void> {
    if (!this.auth.anilist_logged_in) return;
    this.session = await this.api.call<Session>('refresh_lists');
    await this.loadLists();
  }

  private async loadMapping(): Promise<void> {
    const data = await this.api.call<{ reviews: ReviewItem[]; mappings: Mapping[] }>('mapping_data');
    this.reviews = data.reviews;
    this.mappings = data.mappings;
    this.mappingSelected = clamp(this.mappingSelected, this.mappingCount());
  }

  private async loadSyncStatus(): Promise<void> {
    const status = await this.api.call<{ pending: OutboxItem[]; log: SyncLogEntry[] }>('sync_status');
    this.pending = status.pending;
    this.log = status.log;
  }

  private async loadDiff(): Promise<void> {
    if (this.auth.jellyfin_user === null || !this.auth.anilist_logged_in) return;
    this.diff = await this.api.call<SyncRow[]>('sync_diff');
    this.diffSelected = clamp(this.diffSelected, this.diff.length);
  }

  private loadJellyfinData(): void {
    this.run(this.loadHome());
    this.run(this.loadLibrary());
    this.run(this.loadChoices());
  }

  async start(): Promise<void> {
    await this.loadSession();
    this.rows = await this.api.call<LibraryRow[]>('cached_library');
    await this.loadLists();
    this.run(this.loadMapping());
    this.run(this.loadSyncStatus());
    this.loadJellyfinData();
    this.run(this.refreshLists());
    if (this.session?.setup_needed) this.beginSetup();
    else if (this.auth.jellyfin_user === null) this.openJellyfinLogin();
    this.emit();
  }

  private afterLogin(text: string): Promise<void> {
    this.setNotice(true, text);
    return this.loadSession().then(() => {
      this.loadJellyfinData();
      this.run(this.refreshLists());
      this.closeLoginForm();
      this.advanceSetupAfterLogin();
    });
  }

  private closeLoginForm(): void {
    if (this.setup !== null) return;
    if (this.modal?.type !== 'form') return;
    const type = this.modal.purpose.type;
    const done = (type === 'login_jellyfin' && this.auth.jellyfin_user !== null)
      || (type === 'login_anilist' && this.auth.anilist_logged_in);
    if (done) this.modal = null;
  }

  onCoreEvent(event: CoreEvent): void {
    switch (event.type) {
      case 'Playback':
        this.playback = event.data;
        break;
      case 'PlaybackEnded':
        this.playback = null;
        this.run(this.loadHome());
        break;
      case 'LibraryChanged':
        this.run(this.loadHome());
        this.run(this.loadLibrary());
        if (this.series) this.run(this.loadEpisodes(this.series.item.id));
        break;
      case 'ListChanged':
        this.run(this.loadLists());
        break;
      case 'MappingsChanged':
        this.run(this.loadMapping());
        break;
      case 'MatchProgress':
        this.matchProgress = event.data.done < event.data.total ? [event.data.done, event.data.total] : null;
        break;
      case 'Notice':
        this.setNotice(event.data.ok, event.data.message);
        this.run(this.loadSyncStatus());
        break;
      case 'RateLimited':
        this.rateLimited = event.data?.secs ?? null;
        break;
      case 'AuthRequired':
        this.setNotice(false, `${event.data} session expired, please log in again`);
        break;
      case 'TokenExpiring':
        this.setNotice(false, `AniList token expires in ${event.data.days} days, please log in again`);
        break;
    }
    this.emit();
  }

  onTick(): void {
    this.now = Math.floor(Date.now() / 1000);
    if (this.notice && Date.now() - this.notice.at > NOTICE_SECONDS * 1000) this.notice = null;
    this.emit();
  }

  // Tasten ---------------------------------------------------------------------------

  onKey(key: KeyEvent): void {
    if (key.name === 'ctrl-c') {
      this.shouldQuit = true;
    } else if (this.modal) {
      this.onModalKey(this.modal, key);
    } else if (this.mode !== 'normal') {
      this.onInputKey(key);
    } else {
      const action = this.keymap.action(key.name);
      if (action) this.onAction(action);
    }
    this.emit();
  }

  private onInputKey(key: KeyEvent): void {
    if (key.name === 'esc') {
      this.input = '';
      if (this.mode === 'filter') this.applyFilter();
      this.mode = 'normal';
      return;
    }
    if (key.name === 'enter') {
      const command = this.mode === 'command' ? this.input : null;
      this.mode = 'normal';
      if (command !== null) {
        this.input = '';
        this.runCommand(command);
      }
      return;
    }
    if (key.name === 'backspace') this.input = [...this.input].slice(0, -1).join('');
    else if (key.name === 'ctrl-u') this.input = '';
    else if (key.text) this.input += key.text;
    if (this.mode === 'filter') this.applyFilter();
  }

  private applyFilter(): void {
    if (this.tab === 'anilist') {
      this.listFilter = this.input;
      this.listSelected = 0;
    } else {
      this.libraryFilter = this.input;
      this.librarySelected = 0;
    }
  }

  private startFilter(current: string): void {
    this.input = current;
    this.mode = 'filter';
  }

  private switchTab(tab: Tab): void {
    this.tab = tab;
    if (tab === 'sync') {
      this.run(this.loadDiff());
      this.run(this.loadSyncStatus());
    } else if (tab === 'mapping') {
      this.run(this.loadMapping());
    } else if (tab === 'home') {
      this.run(this.loadHome());
    }
  }

  private tabOffset(delta: number): Tab {
    const index = TABS.indexOf(this.tab);
    return TABS[(index + delta + TABS.length) % TABS.length]!;
  }

  private player(command: PlayerCommand): void {
    this.run(this.api.call('player', command));
  }

  private onGlobalAction(action: Action): boolean {
    switch (action) {
      case 'quit': this.shouldQuit = true; return true;
      case 'help': this.modal = { type: 'help' }; return true;
      case 'next_tab': this.switchTab(this.tabOffset(1)); return true;
      case 'prev_tab': this.switchTab(this.tabOffset(-1)); return true;
      case 'tab_home': this.switchTab('home'); return true;
      case 'tab_library': this.switchTab('library'); return true;
      case 'tab_anilist': this.switchTab('anilist'); return true;
      case 'tab_mapping': this.switchTab('mapping'); return true;
      case 'tab_sync': this.switchTab('sync'); return true;
      case 'command': this.input = ''; this.mode = 'command'; return true;
      case 'player_pause': this.player('TogglePause'); return true;
      case 'player_back': this.player({ SeekRelative: -SEEK_STEP }); return true;
      case 'player_forward': this.player({ SeekRelative: SEEK_STEP }); return true;
      case 'player_next': this.player('Next'); return true;
      case 'player_previous': this.player('Previous'); return true;
      case 'player_stop': this.player('Stop'); return true;
      case 'player_subtitle': this.player('CycleSubtitle'); return true;
      case 'player_audio': this.player('CycleAudio'); return true;
      default: return false;
    }
  }

  private motion(action: Action, selected: number, count: number): number | null {
    const last = Math.max(count - 1, 0);
    switch (action) {
      case 'down': return Math.min(selected + 1, last);
      case 'up': return Math.max(selected - 1, 0);
      case 'top': return 0;
      case 'bottom': return last;
      case 'page_down': return Math.min(selected + PAGE, last);
      case 'page_up': return Math.max(selected - PAGE, 0);
      default: return null;
    }
  }

  private onAction(action: Action): void {
    if (this.onGlobalAction(action)) return;
    switch (this.tab) {
      case 'home': this.onHomeAction(action); break;
      case 'library': this.onLibraryAction(action); break;
      case 'anilist': this.onListAction(action); break;
      case 'mapping': this.onMappingAction(action); break;
      case 'sync': this.onSyncAction(action); break;
    }
  }

  private play(libraryId: string, episodeId: string | null): void {
    this.run(this.api.call('play', { library_id: libraryId, episode_id: episodeId }));
  }

  private openShelfDetails(item: ShelfItem): void {
    this.run(this.api.call<LibraryItem>('library_item', { id: item.library_id }).then((library) => {
      this.tab = 'library';
      this.openItem(library);
    }));
  }

  private openItem(item: LibraryItem): void {
    if (item.kind === 'Movie') {
      this.play(item.id, null);
      return;
    }
    this.series = { item, episodes: [], selected: 0 };
    this.run(this.loadEpisodes(item.id));
  }

  private onHomeAction(action: Action): void {
    const rows = this.homeRows();
    const moved = this.motion(action, this.homeSelected, rows.length);
    if (moved !== null) {
      this.homeSelected = moved;
      return;
    }
    const current = rows[this.homeSelected]?.item;
    switch (action) {
      case 'refresh': this.run(this.loadHome()); break;
      case 'open':
        if (!current) break;
        if (current.is_episode) this.play(current.library_id, current.id);
        else this.openShelfDetails(current);
        break;
      case 'play':
      case 'continue':
        if (current) this.play(current.library_id, current.is_episode ? current.id : null);
        break;
      case 'details':
        if (current) this.openShelfDetails(current);
        break;
    }
  }

  private selectedRow(): LibraryRow | undefined {
    return this.visibleLibrary()[this.librarySelected];
  }

  private currentEntry(): ListEntry | null {
    if (this.tab === 'anilist') return this.visibleList()[this.listSelected] ?? null;
    if (this.tab === 'library') {
      const id = this.series?.item.id ?? this.selectedRow()?.item.id;
      return this.rows.find((r) => r.item.id === id)?.entry ?? null;
    }
    return null;
  }

  private onLibraryAction(action: Action): void {
    if (this.series) {
      this.onSeriesAction(this.series, action);
      return;
    }
    const moved = this.motion(action, this.librarySelected, this.visibleLibrary().length);
    if (moved !== null) {
      this.librarySelected = moved;
      return;
    }
    const row = this.selectedRow();
    switch (action) {
      case 'filter': this.startFilter(this.libraryFilter); break;
      case 'back': this.libraryFilter = ''; break;
      case 'refresh': this.run(this.loadLibrary()); this.run(this.loadChoices()); break;
      case 'server_filter': this.openFilterForm(); break;
      case 'open':
      case 'details':
        if (row) this.openItem(row.item);
        break;
      case 'play':
      case 'continue':
        if (row) this.play(row.item.id, null);
        break;
      case 'toggle_played':
        if (row) this.run(this.api.call('set_played', { id: row.item.id, played: !row.item.played }));
        break;
      case 'map':
        if (row) this.openMapForm(row.item);
        break;
      case 'match_all': this.run(this.api.call('match_all')); break;
      case 'increment': case 'decrement': case 'status': case 'score': case 'edit':
        this.onEntryAction(action);
        break;
    }
  }

  private onSeriesAction(series: SeriesView, action: Action): void {
    const moved = this.motion(action, series.selected, series.episodes.length);
    if (moved !== null) {
      series.selected = moved;
      return;
    }
    const episode = series.episodes[series.selected]?.episode;
    switch (action) {
      case 'back': this.series = null; break;
      case 'open':
      case 'play':
        if (episode) this.play(series.item.id, episode.id);
        break;
      case 'continue': this.play(series.item.id, null); break;
      case 'toggle_played':
        if (episode) this.run(this.api.call('set_played', { id: episode.id, played: !episode.played }));
        break;
      case 'refresh': this.run(this.loadEpisodes(series.item.id)); break;
      case 'map': this.openMapForm(series.item); break;
      case 'increment': case 'decrement': case 'status': case 'score': case 'edit':
        this.onEntryAction(action);
        break;
    }
  }

  private save(fields: Record<string, unknown>): Promise<unknown> {
    return this.api.call('save_entry', fields);
  }

  private onEntryAction(action: Action): void {
    const entry = this.currentEntry();
    if (!entry) {
      this.setNotice(false, 'no AniList entry for this title');
      return;
    }
    switch (action) {
      case 'increment': {
        const total = totalUnits(entry.media);
        const next = entry.progress + 1;
        this.run(this.save({ media_id: entry.media.id, progress: total === null ? next : Math.min(next, total) }));
        break;
      }
      case 'decrement':
        this.run(this.save({ media_id: entry.media.id, progress: Math.max(entry.progress - 1, 0) }));
        break;
      case 'status': this.openStatusForm(entry); break;
      case 'score': this.openScoreForm(entry); break;
      case 'edit': this.openEntryForm(entry); break;
    }
  }

  private onListAction(action: Action): void {
    const moved = this.motion(action, this.listSelected, this.visibleList().length);
    if (moved !== null) {
      this.listSelected = moved;
      return;
    }
    switch (action) {
      case 'filter':
        if (this.anilistView !== 'overview') this.startFilter(this.listFilter);
        break;
      case 'back':
        this.listFilter = '';
        this.statusFilter = null;
        break;
      case 'refresh': this.run(this.refreshLists()); break;
      case 'toggle': this.toggleAniListView(); break;
      case 'next_status': this.cycleStatus(true); break;
      case 'prev_status': this.cycleStatus(false); break;
      case 'add':
        this.openForm(new Form(`Add ${this.listKind() === 'ANIME' ? 'Anime' : 'Manga'}`, [
          field.text('query', 'Search AniList'),
        ]), { type: 'search_add' });
        break;
      case 'delete': {
        const entry = this.visibleList()[this.listSelected];
        if (entry) {
          this.modal = {
            type: 'confirm',
            text: `Remove “${preferredTitle(entry.media.title)}” from your list?`,
            run: () => this.run(this.api.call('delete_entry', { media_id: entry.media.id, kind: entry.media.kind })),
          };
        }
        break;
      }
      case 'open': this.onEntryAction('edit'); break;
      case 'edit': case 'increment': case 'decrement': case 'status': case 'score':
        this.onEntryAction(action);
        break;
    }
  }

  private toggleAniListView(): void {
    const order: AniListView[] = ['overview', 'ANIME', 'MANGA'];
    this.anilistView = order[(order.indexOf(this.anilistView) + 1) % order.length]!;
    this.listSelected = 0;
  }

  private cycleStatus(forward: boolean): void {
    if (this.anilistView === 'overview') return;
    const options: (ListStatus | null)[] = [null, ...STATUSES];
    const current = Math.max(options.indexOf(this.statusFilter), 0);
    this.statusFilter = options[(current + (forward ? 1 : options.length - 1)) % options.length]!;
    this.listSelected = 0;
  }

  private onMappingAction(action: Action): void {
    const moved = this.motion(action, this.mappingSelected, this.mappingCount());
    if (moved !== null) {
      this.mappingSelected = moved;
      return;
    }
    if (action === 'toggle') {
      this.mappingView = this.mappingView === 'review' ? 'mapped' : 'review';
      this.mappingSelected = 0;
    } else if (action === 'refresh') {
      this.run(this.loadMapping());
    } else if (action === 'match_all') {
      this.run(this.api.call('match_all'));
    } else if (this.mappingView === 'review' && (action === 'open' || action === 'map')) {
      const review = this.reviews[this.mappingSelected];
      if (!review) return;
      const item = this.rows.find((r) => r.item.id === review.jellyfin_id)?.item;
      if (item) this.openMapForm(item);
      else this.setNotice(false, 'title is not in the loaded library');
    } else if (this.mappingView === 'mapped') {
      const mapping = this.mappings[this.mappingSelected];
      if (!mapping) return;
      if (action === 'edit' || action === 'open') {
        this.openMappingForm(mapping);
      } else if (action === 'delete') {
        this.modal = {
          type: 'confirm',
          text: `Remove all mappings of “${mapping.title}”?`,
          run: () => this.run(this.api.call('unmap', { id: mapping.jellyfin_id })),
        };
      }
    }
  }

  private onSyncAction(action: Action): void {
    const moved = this.motion(action, this.diffSelected, this.diff.length);
    if (moved !== null) {
      this.diffSelected = moved;
      return;
    }
    switch (action) {
      case 'refresh':
        this.run(this.loadDiff());
        this.run(this.loadSyncStatus());
        break;
      case 'delete': {
        const row = this.diff[this.diffSelected];
        if (row) {
          const next: Record<SyncDirection, SyncDirection> = { Push: 'Pull', Pull: 'Skip', Skip: 'Push' };
          row.direction = next[row.direction];
        }
        break;
      }
      case 'apply':
      case 'open': {
        const rows = this.diff.filter((r) => r.direction !== 'Skip');
        if (rows.length > 0) {
          this.modal = {
            type: 'confirm',
            text: `Apply ${rows.length} sync changes?`,
            run: () => this.applySync(rows),
          };
        }
        break;
      }
      case 'add':
        this.run(this.api.call('flush_outbox').then(() => this.loadSyncStatus()));
        break;
    }
  }

  private applySync(rows: SyncRow[]): void {
    this.run(this.api.call<SyncReport>('apply_sync', { rows }).then(async (report) => {
      this.setNotice(
        report.failed === 0,
        `${report.pushed} pushed, ${report.pulled} episodes marked, ${report.conflicts} conflicts, ${report.failed} failed`,
      );
      await this.loadDiff();
      await this.loadSyncStatus();
    }));
  }

  // Befehlszeile ---------------------------------------------------------------------

  runCommand(line: string): void {
    const trimmed = line.trim();
    const space = trimmed.indexOf(' ');
    const command = space < 0 ? trimmed : trimmed.slice(0, space);
    const rest = space < 0 ? '' : trimmed.slice(space + 1).trim();
    const kind = this.listKind();
    const patch = (fields: Record<string, unknown>) => {
      const entry = this.currentEntry();
      if (!entry) this.setNotice(false, 'no AniList entry selected');
      else this.run(this.save({ media_id: entry.media.id, ...fields }));
    };
    switch (command) {
      case '': break;
      case 'q':
      case 'quit': this.shouldQuit = true; break;
      case 'setup': this.beginSetup(); break;
      case 'login':
        if (rest === 'anilist') this.openAniListLogin();
        else this.openJellyfinLogin();
        break;
      case 'logout':
        if (rest === 'jellyfin' || rest === 'anilist') {
          this.run(this.api.call('logout', { service: rest }).then(() => this.afterLogin(`Logged out of ${rest}`)));
        } else {
          this.setNotice(false, 'usage: :logout jellyfin|anilist');
        }
        break;
      case 'status': {
        const status = parseStatus(rest);
        if (status) patch({ status });
        else this.setNotice(false, `unknown status ${rest}`);
        break;
      }
      case 'score': patch({ score: rest }); break;
      case 'progress':
        if (/^\d+$/.test(rest)) patch({ progress: Number(rest) });
        else this.setNotice(false, 'usage: :progress <number>');
        break;
      case 'notes': patch({ notes: rest }); break;
      case 'private': patch({ private: rest !== 'off' }); break;
      case 'sync': this.switchTab('sync'); break;
      case 'flush': this.run(this.api.call('flush_outbox')); break;
      case 'match': this.run(this.api.call('match_all')); break;
      case 'list-create':
        if (rest) this.run(this.listChange('create_list', { kind, name: rest }));
        break;
      case 'list-delete':
        if (rest) this.run(this.listChange('delete_list', { kind, name: rest }));
        break;
      case 'list-rename': {
        const parts = rest.split('->');
        if (parts.length === 2) {
          this.run(this.listChange('rename_list', { kind, old: parts[0]!.trim(), new: parts[1]!.trim() }));
        } else {
          this.setNotice(false, 'usage: :list-rename old name -> new name');
        }
        break;
      }
      default: this.setNotice(false, `unknown command \`${command}\``);
    }
  }

  private async listChange(method: string, params: unknown): Promise<void> {
    this.session = await this.api.call<Session>(method, params);
  }

  // Formulare ------------------------------------------------------------------------

  private openForm(form: Form, purpose: Purpose): void {
    this.modal = { type: 'form', form, purpose };
  }

  openJellyfinLogin(): void {
    this.openForm(new Form('Log in to Jellyfin', [
      field.text('url', 'Server URL', this.auth.jellyfin_url ?? this.session?.jellyfin_url ?? ''),
      field.text('user', 'Username'),
      field.secret('password', 'Password'),
      field.secret('key', 'or API key'),
    ], 'The password is used once and never stored. Esc to skip.'), { type: 'login_jellyfin' });
  }

  private openAniListLogin(): void {
    this.openForm(new Form('Log in to AniList', [
      field.text('client_id', 'Client ID', this.session?.client_id ?? ''),
      field.secret('token', 'Token or redirect URL'),
    ], 'Leave the token empty to log in through the browser.'), { type: 'login_anilist' });
  }

  beginSetup(): void {
    const step: SetupStep = this.auth.jellyfin_user !== null && !this.auth.anilist_logged_in ? 'anilist' : 'jellyfin';
    this.openSetupStep(step);
  }

  private openSetupStep(step: SetupStep): void {
    this.setup = step;
    if (step === 'jellyfin') {
      this.openForm(new Form('Setup 1/3: Jellyfin', [
        field.text('url', 'Server URL', this.auth.jellyfin_url ?? this.session?.jellyfin_url ?? ''),
        field.text('user', 'Username', this.auth.jellyfin_user ?? ''),
        field.secret('password', 'Password'),
        field.secret('key', 'or API key'),
      ], 'Include the port, for example 192.168.1.10:8096.\nThe password is used once and never stored. Esc skips this step.'),
      { type: 'login_jellyfin' });
    } else if (step === 'anilist') {
      this.openForm(new Form('Setup 2/3: AniList', [
        field.text('client_id', 'Client ID', this.session?.client_id ?? ''),
        field.secret('token', 'Token or redirect URL'),
      ], `Create an app at anilist.co/settings/developer\nwith the redirect URL ${this.session?.redirect ?? ''}\nLeave the token empty to log in through the browser. Esc skips.`),
      { type: 'login_anilist' });
    } else {
      this.openForm(new Form('Setup 3/3: Sync', [
        field.toggle('auto_sync', 'Update AniList automatically while watching', this.session?.auto_sync ?? true),
      ], 'When off, AniList only changes when you apply the Sync tab.\nEnter finishes the setup.'),
      { type: 'setup_sync' });
    }
  }

  private advanceSetupAfterLogin(): void {
    if (this.setup === 'jellyfin' && this.auth.jellyfin_user !== null) this.openSetupStep('anilist');
    else if (this.setup === 'anilist' && this.auth.anilist_logged_in) this.openSetupStep('sync');
  }

  private skipSetupStep(purpose: Purpose): void {
    if (this.setup === 'jellyfin' && purpose.type === 'login_jellyfin') {
      this.openSetupStep('anilist');
    } else if (this.setup === 'anilist' && purpose.type === 'login_anilist') {
      this.openSetupStep('sync');
    } else if (this.setup === 'sync' && purpose.type === 'setup_sync') {
      this.setup = null;
      this.run(this.api.call('finish_setup', { auto_sync: null }));
    }
  }

  private statusField(current: ListStatus, kind: MediaKind) {
    return field.choice('status', 'Status', STATUSES.map((s) => statusLabel(s, kind)), statusLabel(current, kind));
  }

  private scoreHint(): string {
    return this.session?.score_hint ?? '';
  }

  private shownScore(entry: ListEntry): string {
    return (entry as ListRow).score_text ?? '';
  }

  private openStatusForm(entry: ListEntry): void {
    this.openForm(new Form(preferredTitle(entry.media.title), [
      this.statusField(entry.status, entry.media.kind),
    ]), { type: 'status', entry });
  }

  private openScoreForm(entry: ListEntry): void {
    this.openForm(new Form(preferredTitle(entry.media.title), [
      field.text('score', `Score (${this.scoreHint()})`, this.shownScore(entry)),
    ]), { type: 'score', entry });
  }

  private advancedNames(kind: MediaKind): string[] {
    const viewer = this.session?.viewer;
    if (!viewer) return [];
    return kind === 'ANIME' ? viewer.anime_advanced_scoring : viewer.manga_advanced_scoring;
  }

  private openEntryForm(entry: ListEntry): void {
    const total = totalUnits(entry.media) ?? '?';
    const fields = [
      this.statusField(entry.status, entry.media.kind),
      field.text('score', `Score (${this.scoreHint()})`, this.shownScore(entry)),
      field.text('progress', `Progress (of ${total})`, String(entry.progress)),
    ];
    if (entry.media.kind === 'MANGA') fields.push(field.text('volumes', 'Volumes', String(entry.progress_volumes)));
    const advanced = this.advancedNames(entry.media.kind);
    if (advanced.length > 0) {
      fields.push(field.text('advanced', 'Advanced scores', advanced.map((n) => `${n}=${advancedValue(entry, n)}`).join(', ')));
    }
    fields.push(
      field.text('repeat', 'Repeats', String(entry.repeat)),
      field.text('priority', 'Priority', String(entry.priority)),
      field.text('started', 'Started (YYYY-MM-DD)', formatDate(entry.started_at)),
      field.text('completed', 'Completed (YYYY-MM-DD)', formatDate(entry.completed_at)),
      field.toggle('private', 'Private', entry.private),
      field.toggle('hidden', 'Hide from status lists', entry.hidden_from_status_lists),
      field.text('lists', 'Custom lists (comma separated)', entry.custom_lists.join(', ')),
      field.text('notes', 'Notes', entry.notes),
    );
    this.openForm(new Form(`Edit: ${preferredTitle(entry.media.title)}`, fields), { type: 'edit_entry', entry });
  }

  private openFilterForm(): void {
    const q = this.query;
    const library = this.libraries.find((l) => l.id === q.library_id)?.name ?? 'All';
    const kind = q.kind === 'Series' ? 'Series' : q.kind === 'Movie' ? 'Movies' : 'All';
    const sort = SORT_LABELS.find(([value]) => value === q.sort)?.[1] ?? 'Name';
    this.openForm(new Form('Library filters', [
      field.choice('library', 'Library', ['All', ...this.libraries.map((l) => l.name)], library),
      field.text('search', 'Search', q.search ?? ''),
      field.text('genres', 'Genres (comma separated)', q.genres.join(', ')),
      field.text('years', 'Years (comma separated)', q.years.join(', ')),
      field.choice('status', 'Status', ['Any', 'Continuing', 'Ended'], q.status ?? 'Any'),
      field.choice('kind', 'Type', ['All', 'Series', 'Movies'], kind),
      field.toggle('unplayed', 'Unplayed only', q.unplayed_only),
      field.choice('sort', 'Sort', SORT_LABELS.map(([, label]) => label), sort),
    ], this.filters.genres.length > 0 ? `Genres: ${this.filters.genres.join(', ')}` : null), { type: 'filters' });
  }

  private openMapForm(item: LibraryItem): void {
    const review = this.reviews.find((r) => r.jellyfin_id === item.id);
    const first = review?.candidates[0];
    const hint = review?.candidates.map((c) => `${c.media_id}: ${c.title} (${c.score.toFixed(2)})`).join('\n') ?? null;
    this.openForm(new Form(`Map: ${item.name}`, [
      field.text('media', 'AniList ID', first ? String(first.media_id) : ''),
      field.toggle('follow', 'Assign sequels to later seasons', true),
    ], hint), { type: 'map_manual', item });
  }

  private openMappingForm(mapping: Mapping): void {
    this.openForm(new Form(`Mapping: ${mapping.title}`, [
      field.text('media', 'AniList ID', String(mapping.media_id)),
      field.text('season', 'Season', String(mapping.season)),
      field.text('start', 'First episode', String(mapping.ep_start)),
      field.text('end', 'Last episode (empty = open)', mapping.ep_end === null ? '' : String(mapping.ep_end)),
      field.text('offset', 'Offset (AniList = Jellyfin + offset)', String(mapping.offset)),
    ]), { type: 'edit_mapping', mapping });
  }

  private onModalKey(modal: Modal, key: KeyEvent): void {
    switch (modal.type) {
      case 'help':
        this.modal = null;
        break;
      case 'confirm':
        if (key.name === 'y' || key.name === 'enter') {
          this.modal = null;
          modal.run();
        } else if (key.name === 'n' || key.name === 'esc') {
          this.modal = null;
        }
        break;
      case 'results': {
        const action = this.keymap.action(key.name);
        if (action === 'back' || action === 'quit') {
          this.modal = null;
        } else if (action === 'open') {
          const media = modal.results[modal.selected];
          if (media) {
            this.openForm(new Form(`Add: ${preferredTitle(media.title)}`, [
              this.statusField('PLANNING', media.kind),
            ]), { type: 'add_status', media });
          }
        } else if (action) {
          const moved = this.motion(action, modal.selected, modal.results.length);
          if (moved !== null) modal.selected = moved;
        }
        break;
      }
      case 'form': {
        const outcome = modal.form.onKey(key);
        if (outcome === 'cancel') {
          this.modal = null;
          this.skipSetupStep(modal.purpose);
        } else if (outcome === 'submit') {
          modal.form.error = null;
          try {
            this.submitForm(modal.form, modal.purpose);
          } catch (error) {
            modal.form.error = error instanceof Error ? error.message : String(error);
          }
        }
        break;
      }
    }
  }

  private parseStatusField(form: Form, kind: MediaKind): ListStatus {
    const status = STATUSES.find((s) => statusLabel(s, kind) === form.value('status'));
    if (!status) throw new Error(`unknown status ${form.value('status')}`);
    return status;
  }

  /** Formular schließen, Arbeit starten und Fehler im Formular wieder anzeigen. */
  private closeAndRun(work: Promise<unknown>): void {
    this.modal = null;
    this.run(work);
  }

  private submitForm(form: Form, purpose: Purpose): void {
    switch (purpose.type) {
      case 'login_jellyfin': {
        const url = form.value('url').trim();
        const user = form.value('user').trim();
        if (url === '' || user === '') throw new Error('server URL and username are required');
        this.run(this.api.call<string>('login_jellyfin', {
          url, user, password: form.value('password'), api_key: form.value('key'),
        }).then((name) => this.afterLogin(`Logged in to Jellyfin as ${name}`)));
        break;
      }
      case 'login_anilist': {
        // In der Einrichtung bleibt das Formular offen, damit Fehler dort erscheinen.
        const keepOpen = this.setup !== null;
        const finish = (name: string) => this.afterLogin(`Logged in to AniList as ${name}`);
        const token = form.value('token').trim();
        if (token !== '') {
          if (!keepOpen) this.modal = null;
          this.run(this.api.call<string>('login_anilist_pin', { input: token }).then(finish));
          break;
        }
        const clientId = form.value('client_id').trim();
        if (clientId === '') throw new Error('enter a client ID or paste a token');
        this.setNotice(true, 'Waiting for the browser…');
        if (!keepOpen) this.modal = null;
        this.run(this.api.call<string>('login_anilist_browser', { client_id: clientId }).then(finish));
        break;
      }
      case 'setup_sync': {
        const autoSync = form.isOn('auto_sync');
        this.setup = null;
        this.setNotice(true, 'Setup complete');
        this.closeAndRun(this.api.call('finish_setup', { auto_sync: autoSync }));
        break;
      }
      case 'status':
        this.closeAndRun(this.save({
          media_id: purpose.entry.media.id,
          status: this.parseStatusField(form, purpose.entry.media.kind),
        }));
        break;
      case 'score':
        this.closeAndRun(this.save({ media_id: purpose.entry.media.id, score: form.value('score') }));
        break;
      case 'edit_entry':
        this.closeAndRun(this.save(this.patchFromForm(form, purpose.entry)));
        break;
      case 'filters':
        this.applyFilterForm(form);
        this.closeAndRun(this.loadLibrary());
        break;
      case 'map_manual': {
        const media = form.value('media').trim();
        if (!/^\d+$/.test(media)) throw new Error('AniList ID must be a number');
        this.closeAndRun(this.api.call('map_manually', {
          item_id: purpose.item.id, media_id: Number(media), follow: form.isOn('follow'),
        }));
        break;
      }
      case 'edit_mapping': {
        const updated = mappingFromForm(form, purpose.mapping);
        const old = purpose.mapping;
        const all = this.mappings.filter((m) => m.jellyfin_id === old.jellyfin_id);
        const index = all.findIndex((m) => m.season === old.season && m.ep_start === old.ep_start);
        if (index >= 0) all[index] = updated;
        else all.push(updated);
        this.closeAndRun(this.api.call('set_mappings', { id: old.jellyfin_id, mappings: all }));
        break;
      }
      case 'search_add': {
        const query = form.value('query').trim();
        if (query === '') throw new Error('enter a title');
        this.closeAndRun(this.api.call<Media[]>('search', { text: query, kind: this.listKind() }).then((results) => {
          this.modal = { type: 'results', results, selected: 0 };
        }));
        break;
      }
      case 'add_status':
        this.closeAndRun(this.api.call('add_to_list', {
          media: purpose.media,
          status: this.parseStatusField(form, purpose.media.kind),
        }));
        break;
    }
  }

  private patchFromForm(form: Form, entry: ListEntry): Record<string, unknown> {
    const patch: Record<string, unknown> = { media_id: entry.media.id };
    const status = this.parseStatusField(form, entry.media.kind);
    if (status !== entry.status) patch.status = status;
    if (form.value('score').trim() !== this.shownScore(entry)) patch.score = form.value('score');
    if (form.has('advanced')) {
      const names = this.advancedNames(entry.media.kind);
      const values = parseAdvanced(form.value('advanced'), names, entry);
      if (names.some((name, i) => advancedValue(entry, name) !== values[i])) patch.advanced_scores = values;
    }
    const number = (key: string, name: string, current: number, target: string) => {
      const text = form.value(key).trim();
      if (text === '') return;
      const value = wholeNumber(text, name);
      if (value !== current) patch[target] = value;
    };
    number('progress', 'progress', entry.progress, 'progress');
    if (form.has('volumes')) number('volumes', 'volumes', entry.progress_volumes, 'progress_volumes');
    number('repeat', 'repeats', entry.repeat, 'repeat');
    number('priority', 'priority', entry.priority, 'priority');
    if (form.value('started') !== formatDate(entry.started_at)) patch.started_at = form.value('started');
    if (form.value('completed') !== formatDate(entry.completed_at)) patch.completed_at = form.value('completed');
    if (form.isOn('private') !== entry.private) patch.private = form.isOn('private');
    if (form.isOn('hidden') !== entry.hidden_from_status_lists) patch.hidden_from_status_lists = form.isOn('hidden');
    const lists = splitList(form.value('lists'));
    if (lists.length !== entry.custom_lists.length || !lists.every((l) => entry.custom_lists.includes(l))) {
      patch.custom_lists = lists;
    }
    if (form.value('notes') !== entry.notes) patch.notes = form.value('notes');
    return patch;
  }

  private applyFilterForm(form: Form): void {
    const years = splitList(form.value('years')).map((text) => {
      if (!/^\d+$/.test(text)) throw new Error(`\`${text}\` is not a year`);
      return Number(text);
    });
    const search = form.value('search').trim();
    const status = form.value('status');
    const kindText = form.value('kind');
    const kind: ItemKind | null = kindText === 'Series' ? 'Series' : kindText === 'Movies' ? 'Movie' : null;
    this.query = {
      library_id: this.libraries.find((l) => l.name === form.value('library'))?.id ?? null,
      search: search === '' ? null : search,
      genres: splitList(form.value('genres')),
      years,
      status: status === 'Any' ? null : status,
      kind,
      unplayed_only: form.isOn('unplayed'),
      sort: SORT_LABELS.find(([, label]) => label === form.value('sort'))?.[0] ?? 'Name',
      start: 0,
      limit: null,
    };
  }
}
