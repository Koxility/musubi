// Bibliothek und Episodenliste einer Serie.

import { Box, Text } from 'ink';
import type { Store, SeriesView } from '../store';
import { statusLabel, totalUnits, type EpisodeRow, type LibraryRow } from '../types';
import { ACCENT, Empty, MUTED, Panel, SELECTED, WIDE_LAYOUT, Window } from './common';

function entrySummary(row: LibraryRow): string {
  if (!row.entry) return '';
  const total = totalUnits(row.entry.media) ?? '?';
  return `${statusLabel(row.entry.status, row.entry.media.kind)} ${row.entry.progress}/${total}`;
}

function LibraryLine({ row, selected }: { row: LibraryRow; selected: boolean }) {
  const unplayed = row.item.unplayed_count ?? 0;
  return (
    <Box backgroundColor={selected ? SELECTED : undefined} columnGap={1}>
      <Box width={1} flexShrink={0}>
        <Text color="yellow">{row.needs_review ? '!' : ' '}</Text>
      </Box>
      <Box flexGrow={1} flexShrink={1} flexBasis={0}>
        <Text wrap="truncate-end" bold={selected}>{row.item.name}</Text>
      </Box>
      <Box width={4} flexShrink={0}><Text color={MUTED}>{row.item.year ?? ''}</Text></Box>
      <Box width={4} flexShrink={0} justifyContent="flex-end">
        <Text color={ACCENT}>{unplayed > 0 ? unplayed : row.item.played ? '✓' : ''}</Text>
      </Box>
      <Box width={18} flexShrink={0}><Text color={MUTED} wrap="truncate-end">{entrySummary(row)}</Text></Box>
    </Box>
  );
}

function ItemDetails({ row, height }: { row: LibraryRow; height: number }) {
  const item = row.item;
  const facts = [
    item.year,
    item.community_rating === null ? null : `★ ${item.community_rating.toFixed(1)}`,
    item.status,
  ].filter(Boolean).join('  ·  ');
  return (
    <Panel title={item.name} flexGrow={2} flexBasis={0} height={height}>
      <Text color={MUTED}>{facts}</Text>
      <Text color={MUTED} wrap="truncate-end">{item.genres.join(', ')}</Text>
      {row.entry ? <Text color={ACCENT}>AniList: {entrySummary(row)}</Text> : null}
      <Box height={1} />
      <Text wrap="wrap">{item.overview ?? ''}</Text>
    </Panel>
  );
}

function EpisodeLine({ row, selected }: { row: EpisodeRow; selected: boolean }) {
  const episode = row.episode;
  return (
    <Box backgroundColor={selected ? SELECTED : undefined} columnGap={1}>
      <Box width={8} flexShrink={0}>
        <Text color={MUTED}>S{episode.season_number}E{String(episode.number).padStart(2, '0')}</Text>
      </Box>
      <Box flexGrow={1} flexShrink={1} flexBasis={0}>
        <Text wrap="truncate-end" bold={selected}>{episode.name}</Text>
      </Box>
      <Box width={7} flexShrink={0}>
        <Text color={MUTED}>{episode.runtime_secs ? `${Math.floor(episode.runtime_secs / 60)} min` : ''}</Text>
      </Box>
      <Box width={2} flexShrink={0}><Text color={ACCENT}>{episode.played ? '✓' : ''}</Text></Box>
      <Box width={2} flexShrink={0}><Text color="green">{row.counted_on_anilist ? 'AL' : ''}</Text></Box>
    </Box>
  );
}

function Series({ series, columns, height }: { series: SeriesView; columns: number; height: number }) {
  const current = series.episodes[series.selected]?.episode;
  return (
    <Box flexGrow={1}>
      <Panel title={series.item.name} flexGrow={3} flexBasis={0} height={height}>
        {series.episodes.length === 0 ? <Empty text="Loading episodes…" /> : null}
        <Window
          items={series.episodes}
          selected={series.selected}
          height={height - 3}
          render={(row, selected, index) => <EpisodeLine key={index} row={row} selected={selected} />}
        />
      </Panel>
      {current && columns >= WIDE_LAYOUT ? (
        <Panel title={current.name} flexGrow={2} flexBasis={0} height={height}>
          <Text wrap="wrap">{current.overview ?? ''}</Text>
          <Box flexGrow={1} />
          <Text color={MUTED} wrap="truncate-end">Enter play   c continue   w watched   esc back</Text>
        </Panel>
      ) : null}
    </Box>
  );
}

export function LibraryView({ store, columns, height }: { store: Store; columns: number; height: number }) {
  if (store.series) return <Series series={store.series} columns={columns} height={height} />;
  const rows = store.visibleLibrary();
  const current = rows[store.librarySelected];
  const title = `Library (${rows.length})${store.libraryFilter ? `  /${store.libraryFilter}` : ''}`;
  return (
    <Box flexGrow={1}>
      <Panel title={title} flexGrow={3} flexBasis={0} height={height}>
        {rows.length === 0 ? <Empty text="No titles match." /> : null}
        <Window
          items={rows}
          selected={store.librarySelected}
          height={height - 3}
          render={(row, selected) => <LibraryLine key={row.item.id} row={row} selected={selected} />}
        />
      </Panel>
      {current && columns >= WIDE_LAYOUT ? <ItemDetails row={current} height={height} /> : null}
    </Box>
  );
}
