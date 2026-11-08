// Startseite wie in einem Mediacenter: Regale links, ausgewählter Titel rechts.

import { Box, Text } from 'ink';
import type { Store } from '../store';
import { progressBar } from '../text';
import type { ShelfItem } from '../types';
import { ACCENT, Empty, MUTED, Panel, SELECTED, SectionWindow, WIDE_LAYOUT } from './common';

function progressOf(item: ShelfItem): number | null {
  if (!item.runtime_secs || item.position_secs <= 0) return null;
  return Math.min(item.position_secs / item.runtime_secs, 1);
}

function ShelfLine({ item, selected }: { item: ShelfItem; selected: boolean }) {
  const progress = progressOf(item);
  return (
    <Box backgroundColor={selected ? SELECTED : undefined} columnGap={1} flexGrow={1}>
      <Box flexGrow={1} flexShrink={1} flexBasis={0} paddingRight={1}>
        <Text wrap="truncate-end" bold={selected}>{item.title}</Text>
      </Box>
      <Box width={22} flexShrink={0}>
        <Text color={MUTED} wrap="truncate-end">{item.subtitle ?? item.year ?? ''}</Text>
      </Box>
      <Box width={8} flexShrink={0}>
        {progress === null ? null : <Text color={ACCENT}>{progressBar(progress, 8)}</Text>}
      </Box>
    </Box>
  );
}

function Details({ item, height }: { item: ShelfItem; height: number }) {
  const progress = progressOf(item);
  return (
    <Panel title={item.title} flexGrow={2} flexBasis={0} height={height}>
      {item.subtitle ? <Text color={MUTED}>{item.subtitle}</Text> : null}
      <Text color={MUTED}>
        {[item.year, item.runtime_secs ? `${Math.floor(item.runtime_secs / 60)} min` : null].filter(Boolean).join('  ·  ')}
      </Text>
      {progress === null ? null : <Text color={ACCENT}>{progressBar(progress, 24)}</Text>}
      <Box height={1} />
      <Text wrap="wrap">{item.overview ?? ''}</Text>
      <Box flexGrow={1} />
      <Text color={MUTED} wrap="truncate-end">Enter {item.is_episode ? 'play' : 'open'}   p play   i details</Text>
    </Panel>
  );
}

export function HomeView({ store, columns, height }: { store: Store; columns: number; height: number }) {
  const rows = store.homeRows();
  const current = rows[store.homeSelected]?.item;
  const empty = store.auth.jellyfin_user === null ? 'Log in to Jellyfin with :login.' : 'Nothing to show yet.';
  return (
    <Box flexGrow={1}>
      <Panel title="Home" flexGrow={3} flexBasis={0} height={height}>
        {rows.length === 0 ? <Empty text={empty} /> : null}
        <SectionWindow
          rows={rows}
          section={(row) => row.section}
          selected={store.homeSelected}
          height={height - 3}
          render={(row, selected) => <ShelfLine item={row.item} selected={selected} />}
        />
      </Panel>
      {current && columns >= WIDE_LAYOUT ? <Details item={current} height={height} /> : null}
    </Box>
  );
}
