// AniList: Übersicht mit Airing und in Progress, dazu die vollständigen Listen.

import { Box, Text } from 'ink';
import type { Store } from '../store';
import { countdown } from '../text';
import { preferredTitle, statusLabel, totalUnits, type ListRow, type ListStatus } from '../types';
import { ACCENT, Empty, MUTED, Panel, SELECTED, SectionWindow, WIDE_LAYOUT, Window } from './common';

const STATUS_COLORS: Record<ListStatus, string> = {
  CURRENT: 'green', REPEATING: 'greenBright', COMPLETED: 'blue',
  PAUSED: 'yellow', DROPPED: 'red', PLANNING: 'magenta',
};

function progressText(entry: ListRow): string {
  return `${entry.progress}/${totalUnits(entry.media) ?? '?'}`;
}

function OverviewLine({ entry, selected, now }: { entry: ListRow; selected: boolean; now: number }) {
  const next = entry.media.next_airing;
  return (
    <Box backgroundColor={selected ? SELECTED : undefined} columnGap={1} flexGrow={1}>
      <Box flexGrow={1} flexShrink={1} flexBasis={0}>
        <Text wrap="truncate-end" bold={selected}>{preferredTitle(entry.media.title)}</Text>
      </Box>
      <Box width={18} flexShrink={0}>
        {next ? <Text color={ACCENT}>Ep {next.episode} in {countdown(next.airing_at - now)}</Text> : null}
      </Box>
      <Box width={8} flexShrink={0}><Text>{progressText(entry)}</Text></Box>
      <Box width={10} flexShrink={0}>
        {entry.behind > 0 ? <Text color="yellow">{entry.behind} behind</Text> : null}
      </Box>
    </Box>
  );
}

function ListLine({ entry, selected }: { entry: ListRow; selected: boolean }) {
  return (
    <Box backgroundColor={selected ? SELECTED : undefined} columnGap={1}>
      <Box flexGrow={1} flexShrink={1} flexBasis={0}>
        <Text wrap="truncate-end" bold={selected}>{preferredTitle(entry.media.title)}</Text>
      </Box>
      <Box width={11} flexShrink={0}>
        <Text color={STATUS_COLORS[entry.status]}>{statusLabel(entry.status, entry.media.kind)}</Text>
      </Box>
      <Box width={6} flexShrink={0}><Text>{entry.score_text || '–'}</Text></Box>
      <Box width={9} flexShrink={0}><Text>{progressText(entry)}</Text></Box>
      <Box width={6} flexShrink={0}><Text color={MUTED}>{entry.media.format ?? ''}</Text></Box>
    </Box>
  );
}

function EntryDetails({ entry, height }: { entry: ListRow; height: number }) {
  const media = entry.media;
  const facts = [
    media.format,
    media.season_year,
    media.average_score === null ? null : `${media.average_score}%`,
  ].filter(Boolean).join('  ·  ');
  return (
    <Panel title={preferredTitle(media.title)} flexGrow={2} flexBasis={0} height={height}>
      <Text color={MUTED}>{facts}</Text>
      <Text color={STATUS_COLORS[entry.status]}>
        {statusLabel(entry.status, media.kind)} {progressText(entry)}
      </Text>
      <Text color={MUTED} wrap="truncate-end">{media.genres.join(', ')}</Text>
      <Box height={1} />
      <Text wrap="wrap">{media.description ?? ''}</Text>
      <Box flexGrow={1} />
      <Text color={MUTED} wrap="truncate-end">+ - progress   s status   S score   e edit</Text>
    </Panel>
  );
}

function viewTabs(store: Store): string {
  const labels = [['overview', 'Overview'], ['ANIME', 'Anime'], ['MANGA', 'Manga']] as const;
  return labels.map(([view, label]) => (store.anilistView === view ? `[${label}]` : label)).join('  ');
}

export function AniListView({ store, columns, height }: { store: Store; columns: number; height: number }) {
  if (!store.auth.anilist_logged_in) {
    return (
      <Panel title="AniList" flexGrow={1} height={height}>
        <Empty text="Log in to AniList with :login anilist." />
      </Panel>
    );
  }
  const rows = store.visibleList();
  const current = rows[store.listSelected];
  const filter = [
    store.statusFilter ? statusLabel(store.statusFilter, store.listKind()) : null,
    store.listFilter ? `/${store.listFilter}` : null,
  ].filter(Boolean).join(' ');
  const title = `${viewTabs(store)}   ${filter}`;
  return (
    <Box flexGrow={1}>
      <Panel title={title} flexGrow={3} flexBasis={0} height={height}>
        {rows.length === 0 ? <Empty text="Nothing here yet. Press a to add a title." /> : null}
        {store.anilistView === 'overview' ? (
          <SectionWindow
            rows={store.overviewRows()}
            section={(row) => row.section}
            selected={store.listSelected}
            height={height - 3}
            render={(row, selected) => <OverviewLine entry={row.entry} selected={selected} now={store.now} />}
          />
        ) : (
          <Window
            items={rows}
            selected={store.listSelected}
            height={height - 3}
            render={(entry, selected) => <ListLine key={entry.id} entry={entry} selected={selected} />}
          />
        )}
      </Panel>
      {current && columns >= WIDE_LAYOUT ? <EntryDetails entry={current} height={height} /> : null}
    </Box>
  );
}
