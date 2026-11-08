import { describe, expect, test } from 'bun:test';
import { render } from 'ink-testing-library';
import { Store } from '../src/store';
import type { Api } from '../src/rpc';
import { HomeView } from '../src/ui/Home';
import { AniListView } from '../src/ui/AniList';

const api: Api = { call: async () => undefined as never };

describe('views', () => {
  test('home shows shelves with section headers and details', () => {
    const store = new Store(api);
    store.auth = { jellyfin_user: 'u', jellyfin_url: 'x', anilist_user: null, anilist_logged_in: false };
    store.shelves = {
      continue_watching: [{
        id: 'e', library_id: 's', is_episode: true, title: 'Frieren', subtitle: 'S1:E3 · Pilot',
        overview: 'An elf travels.', year: null, runtime_secs: 1440, position_secs: 720,
      }],
      next_up: [], latest: [],
    };
    const { lastFrame } = render(<HomeView store={store} columns={120} height={12} />);
    const frame = lastFrame() ?? '';
    expect(frame).toContain('Continue Watching');
    expect(frame).toContain('Frieren');
    expect(frame).toContain('An elf travels.');
  });

  test('anilist overview shows the airing countdown and backlog', () => {
    const store = new Store(api);
    store.now = 1000;
    store.auth = { jellyfin_user: null, jellyfin_url: null, anilist_user: 'a', anilist_logged_in: true };
    store.overview.airing = [{
      id: 1, status: 'CURRENT', score_raw: 0, progress: 2, progress_volumes: 0, repeat: 0, priority: 0,
      private: false, hidden_from_status_lists: false, notes: '', custom_lists: [], advanced_scores: [],
      started_at: { year: null, month: null, day: null }, completed_at: { year: null, month: null, day: null },
      updated_at: 0, score_text: '', behind: 5,
      media: {
        id: 1, kind: 'ANIME', title: { romaji: 'Sousou no Frieren', english: null, native: null }, format: 'TV',
        status: 'RELEASING', episodes: 28, chapters: null, volumes: null, season_year: 2023, description: null,
        genres: [], average_score: null, next_airing: { airing_at: 1000 + 3 * 3600, episode: 8 },
      },
    }];
    const { lastFrame } = render(<AniListView store={store} columns={120} height={12} />);
    const frame = lastFrame() ?? '';
    expect(frame).toContain('Airing');
    expect(frame).toContain('Ep 8 in 3h 0m');
    expect(frame).toContain('5 behind');
    expect(frame).toContain('2/28');
  });
});
