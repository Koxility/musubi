// Spiegelt die JSON-Form der Rust-Typen aus musubi-domain und musubi-core.

export type MediaKind = 'ANIME' | 'MANGA';
export type ListStatus = 'CURRENT' | 'PLANNING' | 'COMPLETED' | 'DROPPED' | 'PAUSED' | 'REPEATING';
export type ItemKind = 'Series' | 'Movie';
export type LibrarySort = 'Name' | 'Year' | 'DateAdded' | 'Rating';
export type SyncDirection = 'Push' | 'Pull' | 'Skip';

export interface FuzzyDate {
  year: number | null;
  month: number | null;
  day: number | null;
}

export interface MediaTitle {
  romaji: string | null;
  english: string | null;
  native: string | null;
}

export interface NextAiring {
  airing_at: number;
  episode: number;
}

export interface Media {
  id: number;
  kind: MediaKind;
  title: MediaTitle;
  format: string | null;
  status: string | null;
  episodes: number | null;
  chapters: number | null;
  volumes: number | null;
  season_year: number | null;
  description: string | null;
  genres: string[];
  average_score: number | null;
  next_airing: NextAiring | null;
}

export interface ListEntry {
  id: number;
  media: Media;
  status: ListStatus;
  score_raw: number;
  progress: number;
  progress_volumes: number;
  repeat: number;
  priority: number;
  private: boolean;
  hidden_from_status_lists: boolean;
  notes: string;
  custom_lists: string[];
  advanced_scores: [string, number][];
  started_at: FuzzyDate;
  completed_at: FuzzyDate;
  updated_at: number;
}

/** Listeneintrag samt vom Kern formatierter Punktzahl. */
export interface ListRow extends ListEntry {
  score_text: string;
  behind: number;
}

export interface Overview {
  airing: ListRow[];
  anime_in_progress: ListRow[];
  manga_in_progress: ListRow[];
}

export interface Viewer {
  id: number;
  name: string;
  anime_custom_lists: string[];
  manga_custom_lists: string[];
  anime_advanced_scoring: string[];
  manga_advanced_scoring: string[];
}

export interface AuthStatus {
  jellyfin_user: string | null;
  jellyfin_url: string | null;
  anilist_user: string | null;
  anilist_logged_in: boolean;
}

export interface Session {
  auth: AuthStatus;
  viewer: Viewer | null;
  setup_needed: boolean;
  jellyfin_url: string | null;
  client_id: string | null;
  redirect: string;
  auto_sync: boolean;
  score_hint: string;
  keys: Record<string, string>;
}

export interface LibraryItem {
  id: string;
  kind: ItemKind;
  name: string;
  overview: string | null;
  year: number | null;
  status: string | null;
  genres: string[];
  community_rating: number | null;
  runtime_secs: number | null;
  played: boolean;
  unplayed_count: number | null;
}

export interface Mapping {
  jellyfin_id: string;
  season: number;
  ep_start: number;
  ep_end: number | null;
  media_id: number;
  offset: number;
  source: string;
  confidence: number;
  title: string;
}

export interface LibraryRow {
  item: LibraryItem;
  mappings: Mapping[];
  entry: ListEntry | null;
  needs_review: boolean;
}

export interface Episode {
  id: string;
  season_number: number;
  number: number;
  name: string;
  overview: string | null;
  runtime_secs: number | null;
  played: boolean;
  position_secs: number;
}

export interface EpisodeRow {
  episode: Episode;
  anilist: [number, number] | null;
  counted_on_anilist: boolean;
}

export interface ShelfItem {
  id: string;
  library_id: string;
  is_episode: boolean;
  title: string;
  subtitle: string | null;
  overview: string | null;
  year: number | null;
  runtime_secs: number | null;
  position_secs: number;
}

export interface HomeShelves {
  continue_watching: ShelfItem[];
  next_up: ShelfItem[];
  latest: ShelfItem[];
}

export interface Library {
  id: string;
  name: string;
}

export interface LibraryFilters {
  genres: string[];
  years: number[];
}

export interface LibraryQuery {
  library_id: string | null;
  search: string | null;
  genres: string[];
  years: number[];
  status: string | null;
  kind: ItemKind | null;
  unplayed_only: boolean;
  sort: LibrarySort;
  start: number;
  limit: number | null;
}

export interface ReviewCandidate {
  media_id: number;
  title: string;
  score: number;
}

export interface ReviewItem {
  jellyfin_id: string;
  title: string;
  candidates: ReviewCandidate[];
}

export interface SyncRow {
  series_id: string;
  series_title: string;
  media_id: number;
  media_title: string;
  jellyfin_progress: number;
  anilist_progress: number;
  total: number | null;
  status: ListStatus | null;
  direction: SyncDirection;
}

export interface SyncReport {
  pushed: number;
  pulled: number;
  conflicts: number;
  failed: number;
}

export interface OutboxItem {
  media_id: number;
  attempts: number;
  last_error: string | null;
  failed: boolean;
}

export interface SyncLogEntry {
  at: number;
  ok: boolean;
  message: string;
}

export interface PlaybackState {
  item_id: string;
  title: string;
  position: number;
  duration: number | null;
  paused: boolean;
}

export type CoreEvent =
  | { type: 'Playback'; data: PlaybackState }
  | { type: 'PlaybackEnded' }
  | { type: 'LibraryChanged' }
  | { type: 'ListChanged'; data: MediaKind }
  | { type: 'MappingsChanged' }
  | { type: 'MatchProgress'; data: { done: number; total: number } }
  | { type: 'Notice'; data: { ok: boolean; message: string } }
  | { type: 'RateLimited'; data: { secs: number; nanos: number } | null }
  | { type: 'AuthRequired'; data: 'Jellyfin' | 'AniList' }
  | { type: 'TokenExpiring'; data: { days: number } };

export type PlayerCommand =
  | 'TogglePause'
  | 'CycleSubtitle'
  | 'CycleAudio'
  | 'Next'
  | 'Previous'
  | 'Stop'
  | { SeekRelative: number };

export const STATUSES: ListStatus[] = ['CURRENT', 'REPEATING', 'COMPLETED', 'PAUSED', 'DROPPED', 'PLANNING'];

export function statusLabel(status: ListStatus, kind: MediaKind): string {
  switch (status) {
    case 'CURRENT':
      return kind === 'ANIME' ? 'Watching' : 'Reading';
    case 'REPEATING':
      return kind === 'ANIME' ? 'Rewatching' : 'Rereading';
    case 'PLANNING':
      return 'Planning';
    case 'COMPLETED':
      return 'Completed';
    case 'DROPPED':
      return 'Dropped';
    case 'PAUSED':
      return 'Paused';
  }
}

/** Dieselben Kurzformen wie `ListStatus::from_str` im Kern. */
export function parseStatus(text: string): ListStatus | null {
  const value = text.trim().toLowerCase();
  if (['current', 'watching', 'reading', 'w'].includes(value)) return 'CURRENT';
  if (['planning', 'planned', 'plan', 'p'].includes(value)) return 'PLANNING';
  if (['completed', 'complete', 'done', 'c'].includes(value)) return 'COMPLETED';
  if (['dropped', 'drop', 'd'].includes(value)) return 'DROPPED';
  if (['paused', 'pause', 'hold'].includes(value)) return 'PAUSED';
  if (['repeating', 'rewatching', 'rereading', 'r'].includes(value)) return 'REPEATING';
  return null;
}

export function preferredTitle(title: MediaTitle): string {
  return title.english ?? title.romaji ?? title.native ?? '';
}

export function totalUnits(media: Media): number | null {
  return media.kind === 'ANIME' ? media.episodes : media.chapters;
}

export function formatDate(date: FuzzyDate): string {
  if (date.year === null) return '';
  const parts = [String(date.year).padStart(4, '0')];
  if (date.month !== null) {
    parts.push(String(date.month).padStart(2, '0'));
    if (date.day !== null) parts.push(String(date.day).padStart(2, '0'));
  }
  return parts.join('-');
}
