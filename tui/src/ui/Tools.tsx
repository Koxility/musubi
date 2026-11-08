// Zuordnung und Sync.

import { Box, Text } from 'ink';
import type { Store } from '../store';
import type { Mapping, ReviewItem, SyncDirection, SyncRow } from '../types';
import { ACCENT, Empty, MUTED, Panel, SELECTED, WIDE_LAYOUT, Window } from './common';

function ReviewLine({ review, selected }: { review: ReviewItem; selected: boolean }) {
  return (
    <Box backgroundColor={selected ? SELECTED : undefined} columnGap={1}>
      <Box flexGrow={1} flexShrink={1} flexBasis={0}>
        <Text wrap="truncate-end" bold={selected}>{review.title}</Text>
      </Box>
      <Text color={MUTED}>{review.candidates.length} candidates</Text>
    </Box>
  );
}

function MappedLine({ mapping, selected }: { mapping: Mapping; selected: boolean }) {
  const range = `S${mapping.season} E${mapping.ep_start}-${mapping.ep_end ?? '…'}`;
  const offset = mapping.offset >= 0 ? `+${mapping.offset}` : String(mapping.offset);
  return (
    <Box backgroundColor={selected ? SELECTED : undefined} columnGap={1}>
      <Box flexGrow={1} flexShrink={1} flexBasis={0}>
        <Text wrap="truncate-end" bold={selected}>{mapping.title}</Text>
      </Box>
      <Text color={MUTED}>{range} ({offset})</Text>
    </Box>
  );
}

export function MappingView({ store, height }: { store: Store; height: number }) {
  const review = store.mappingView === 'review';
  const title = review ? `Needs review (${store.reviews.length})` : `Mapped titles (${store.mappings.length})`;
  const count = review ? store.reviews.length : store.mappings.length;
  return (
    <Panel title={`${title}   t switch   M match all`} flexGrow={1} height={height}>
      {count === 0 ? <Empty text={review ? 'Nothing to review.' : 'No mappings yet. Press M to match everything.'} /> : null}
      {review ? (
        <Window
          items={store.reviews}
          selected={store.mappingSelected}
          height={height - 3}
          render={(item, selected) => <ReviewLine key={item.jellyfin_id} review={item} selected={selected} />}
        />
      ) : (
        <Window
          items={store.mappings}
          selected={store.mappingSelected}
          height={height - 3}
          render={(item, selected, index) => <MappedLine key={index} mapping={item} selected={selected} />}
        />
      )}
    </Panel>
  );
}

const ARROWS: Record<SyncDirection, [string, string]> = {
  Push: ['→ AniList', 'green'],
  Pull: ['→ Jellyfin', ACCENT],
  Skip: ['skip', MUTED],
};

function DiffLine({ row, selected }: { row: SyncRow; selected: boolean }) {
  const [label, color] = ARROWS[row.direction];
  return (
    <Box backgroundColor={selected ? SELECTED : undefined} columnGap={1}>
      <Box flexGrow={1} flexShrink={1} flexBasis={0}>
        <Text wrap="truncate-end" bold={selected}>{row.media_title}</Text>
      </Box>
      <Box width={16} flexShrink={0}>
        <Text color={MUTED}>{row.jellyfin_progress} / {row.anilist_progress}/{row.total ?? '?'}</Text>
      </Box>
      <Box width={10} flexShrink={0}><Text color={color}>{label}</Text></Box>
    </Box>
  );
}

export function SyncView({ store, columns, height }: { store: Store; columns: number; height: number }) {
  return (
    <Box flexGrow={1}>
      <Panel title="Differences   d direction   A apply   a retry queue" flexGrow={3} flexBasis={0} height={height}>
        {store.diff.length === 0 ? <Empty text="Jellyfin and AniList agree." /> : null}
        <Window
          items={store.diff}
          selected={store.diffSelected}
          height={height - 3}
          render={(row, selected, index) => <DiffLine key={index} row={row} selected={selected} />}
        />
      </Panel>
      {columns >= WIDE_LAYOUT ? (
        <Panel title="Queue and history" flexGrow={2} flexBasis={0} height={height}>
          {store.pending.slice(0, 5).map((item) => (
            <Text key={item.media_id} color="yellow" wrap="truncate-end">
              Pending {item.media_id}, {item.attempts} attempts{item.last_error ? `: ${item.last_error}` : ''}
            </Text>
          ))}
          {store.log.slice(0, Math.max(height - 8, 1)).map((entry, index) => (
            <Text key={index} color={entry.ok ? undefined : 'red'} wrap="truncate-end">{entry.message}</Text>
          ))}
        </Panel>
      ) : null}
    </Box>
  );
}
