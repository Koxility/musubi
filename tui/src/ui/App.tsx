// Oberfläche: Kopfzeile, aktive Ansicht oder Dialog, Player und Statuszeile.

import { useEffect, useSyncExternalStore } from 'react';
import { Box, Text, useApp, useInput, useWindowSize } from 'ink';
import { displayValue, type Form } from '../form';
import { toKeyEvent } from '../keys';
import { TABS, type Store, type Tab } from '../store';
import { formatTime, progressBar } from '../text';
import { preferredTitle } from '../types';
import { AniListView } from './AniList';
import { HomeView } from './Home';
import { LibraryView } from './Library';
import { MappingView, SyncView } from './Tools';
import { ACCENT, MUTED, Panel, Window } from './common';

const TAB_LABELS: Record<Tab, string> = {
  home: 'Home', library: 'Library', anilist: 'AniList', mapping: 'Mapping', sync: 'Sync',
};

const HELP: [string, string][] = [
  ['1-5, Tab', 'switch tab'],
  ['j/k, g/G, Ctrl-d/u', 'move, jump, page'],
  ['/', 'fuzzy filter the list'],
  ['F', 'server filters: genre, year, status, type'],
  ['Enter, i', 'open or play, show details'],
  ['c, p', 'continue with next unwatched episode'],
  ['w', 'toggle watched in Jellyfin'],
  ['+ -', 'change AniList progress'],
  ['s S e', 'status, score, full editor'],
  ['a d', 'add to list, remove from list'],
  ['t, [ ]', 'overview/anime/manga, status filter'],
  ['m M', 'map title, match everything'],
  ['A', 'apply sync (Sync tab, d changes direction)'],
  ['Space, ←/→', 'pause, seek 10 s'],
  ['n N X', 'next, previous, stop'],
  ['v b', 'cycle subtitles, audio'],
  [':status :score :progress', 'edit selected entry'],
  [':notes :private on|off', 'edit selected entry'],
  [':list-create NAME', 'new custom list'],
  [':list-delete NAME', 'remove custom list'],
  [':list-rename A -> B', 'rename custom list'],
  [':setup', 'guided setup'],
  [':login [anilist]', 'log in'],
  [':logout jellyfin|anilist', 'log out'],
  [':sync :flush :match', 'compare, retry queue, match'],
];

function Header({ store }: { store: Store }) {
  const account = [
    store.auth.jellyfin_user ? `Jellyfin: ${store.auth.jellyfin_user}` : 'Jellyfin: –',
    `AniList: ${store.auth.anilist_user ?? (store.auth.anilist_logged_in ? 'logged in' : '–')}`,
  ].join('  ');
  return (
    <Box height={1} justifyContent="space-between">
      <Box columnGap={2}>
        <Text bold color={ACCENT}>Musubi</Text>
        {TABS.map((tab, index) => (
          <Text key={tab} bold={store.tab === tab} color={store.tab === tab ? ACCENT : MUTED}>
            {index + 1} {TAB_LABELS[tab]}
          </Text>
        ))}
      </Box>
      <Text color={MUTED} wrap="truncate-end">{account}</Text>
    </Box>
  );
}

function PlayerBar({ store, columns }: { store: Store; columns: number }) {
  const state = store.playback;
  if (!state) return null;
  const time = `${formatTime(state.position)} / ${state.duration === null ? '?' : formatTime(state.duration)}`;
  const barWidth = Math.max(columns - time.length - 6, 10);
  return (
    <Box flexDirection="column" height={2}>
      <Text wrap="truncate-end">
        <Text color={ACCENT}>{state.paused ? '⏸' : '▶'} </Text>
        {state.title}
      </Text>
      <Box columnGap={1}>
        <Text color={ACCENT}>{progressBar(state.duration ? state.position / state.duration : 0, barWidth)}</Text>
        <Text color={MUTED}>{time}</Text>
      </Box>
    </Box>
  );
}

function StatusLine({ store }: { store: Store }) {
  if (store.mode !== 'normal') {
    return (
      <Text wrap="truncate-start">
        <Text color={ACCENT}>{store.mode === 'filter' ? '/' : ':'}</Text>
        {store.input}▏
      </Text>
    );
  }
  if (store.notice) {
    return <Text color={store.notice.ok ? 'green' : 'red'} wrap="truncate-end">{store.notice.text}</Text>;
  }
  if (store.rateLimited !== null) {
    return <Text color="yellow">AniList rate limit reached, waiting {store.rateLimited} s</Text>;
  }
  if (store.matchProgress) {
    return <Text color={MUTED}>Matching {store.matchProgress[0]} / {store.matchProgress[1]}</Text>;
  }
  return <Text color={MUTED} wrap="truncate-end">? help   : command   / filter   q quit</Text>;
}

function FormDialog({ form, columns }: { form: Form; columns: number }) {
  const labelWidth = Math.max(...form.fields.map((f) => [...f.label].length));
  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={ACCENT}
      paddingX={1}
      width={Math.min(76, columns - 2)}
    >
      <Text bold>{form.title}</Text>
      <Box height={1} />
      {form.fields.map((target, index) => {
        const focused = index === form.focus;
        const editable = target.kind === 'text' || target.kind === 'secret';
        return (
          <Box key={target.key}>
            <Box width={labelWidth + 4} flexShrink={0}>
              <Text color={focused ? ACCENT : undefined}>{focused ? '▸ ' : '  '}{target.label}</Text>
            </Box>
            <Text wrap="truncate-start">{displayValue(target)}{focused && editable ? '▏' : ''}</Text>
          </Box>
        );
      })}
      {form.error ? <Text color="red">{form.error}</Text> : null}
      {form.hint ? <Box height={1} /> : null}
      {form.hint ? <Text color={MUTED}>{form.hint}</Text> : null}
      <Box height={1} />
      <Text color={MUTED}>Enter save   Tab next   Esc cancel</Text>
    </Box>
  );
}

function ModalView({ store, columns, height }: { store: Store; columns: number; height: number }) {
  const modal = store.modal;
  if (!modal) return null;
  let content;
  switch (modal.type) {
    case 'form':
      content = <FormDialog form={modal.form} columns={columns} />;
      break;
    case 'confirm':
      content = (
        <Box flexDirection="column" borderStyle="round" borderColor={ACCENT} paddingX={1}>
          <Text>{modal.text}</Text>
          <Text color={MUTED}>y confirm   n cancel</Text>
        </Box>
      );
      break;
    case 'results':
      content = (
        <Panel title="Results" width={Math.min(76, columns - 2)}>
          {modal.results.length === 0 ? <Text color={MUTED}>No results.</Text> : null}
          <Window
            items={modal.results}
            selected={modal.selected}
            height={Math.max(height - 4, 3)}
            render={(media, selected, index) => (
              <Box key={index} backgroundColor={selected ? '#1f2b3d' : undefined} columnGap={1}>
                <Box flexGrow={1}><Text wrap="truncate-end">{preferredTitle(media.title)}</Text></Box>
                <Text color={MUTED}>{media.format ?? ''} {media.season_year ?? ''}</Text>
              </Box>
            )}
          />
        </Panel>
      );
      break;
    case 'help':
      content = (
        <Panel title="Keyboard" width={Math.min(76, columns - 2)}>
          {HELP.slice(0, Math.max(height - 3, 3)).map(([keys, text]) => (
            <Box key={keys}>
              <Box width={28} flexShrink={0}><Text color={ACCENT}>{keys}</Text></Box>
              <Text wrap="truncate-end">{text}</Text>
            </Box>
          ))}
        </Panel>
      );
      break;
  }
  return <Box flexGrow={1} justifyContent="center" alignItems="center">{content}</Box>;
}

function Body({ store, columns, height }: { store: Store; columns: number; height: number }) {
  switch (store.tab) {
    case 'home': return <HomeView store={store} columns={columns} height={height} />;
    case 'library': return <LibraryView store={store} columns={columns} height={height} />;
    case 'anilist': return <AniListView store={store} columns={columns} height={height} />;
    case 'mapping': return <MappingView store={store} height={height} />;
    case 'sync': return <SyncView store={store} columns={columns} height={height} />;
  }
}

export function App({ store }: { store: Store }) {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const { exit } = useApp();
  const { columns, rows } = useWindowSize();
  useInput((input, key) => store.onKey(toKeyEvent(input, key)));
  useEffect(() => {
    if (store.shouldQuit) exit();
  });
  const bodyHeight = Math.max(rows - 2 - (store.playback ? 2 : 0), 5);
  return (
    <Box flexDirection="column" width={columns} height={rows}>
      <Header store={store} />
      <Box height={bodyHeight}>
        {store.modal
          ? <ModalView store={store} columns={columns} height={bodyHeight} />
          : <Body store={store} columns={columns} height={bodyHeight} />}
      </Box>
      <PlayerBar store={store} columns={columns} />
      <Box height={1}><StatusLine store={store} /></Box>
    </Box>
  );
}
