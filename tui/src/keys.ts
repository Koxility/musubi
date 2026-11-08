// Tastenbelegung. Standardwerte lassen sich in config.toml unter [tui.keys] überschreiben.

export type Action =
  | 'quit' | 'help' | 'next_tab' | 'prev_tab'
  | 'tab_home' | 'tab_library' | 'tab_anilist' | 'tab_mapping' | 'tab_sync'
  | 'down' | 'up' | 'top' | 'bottom' | 'page_down' | 'page_up'
  | 'open' | 'details' | 'back' | 'filter' | 'command' | 'refresh' | 'server_filter'
  | 'play' | 'continue' | 'toggle_played' | 'increment' | 'decrement'
  | 'status' | 'score' | 'edit' | 'delete' | 'add' | 'toggle'
  | 'next_status' | 'prev_status' | 'map' | 'match_all' | 'apply'
  | 'player_pause' | 'player_back' | 'player_forward' | 'player_next'
  | 'player_previous' | 'player_stop' | 'player_subtitle' | 'player_audio';

const DEFAULTS: [Action, string][] = [
  ['quit', 'q'], ['help', '?'], ['next_tab', 'tab'], ['prev_tab', 'backtab'],
  ['tab_home', '1'], ['tab_library', '2'], ['tab_anilist', '3'], ['tab_mapping', '4'], ['tab_sync', '5'],
  ['down', 'j'], ['up', 'k'], ['top', 'g'], ['bottom', 'G'], ['page_down', 'ctrl-d'], ['page_up', 'ctrl-u'],
  ['open', 'enter'], ['details', 'i'], ['back', 'esc'], ['filter', '/'], ['command', ':'], ['refresh', 'r'],
  ['server_filter', 'F'], ['play', 'p'], ['continue', 'c'], ['toggle_played', 'w'],
  ['increment', '+'], ['decrement', '-'], ['status', 's'], ['score', 'S'], ['edit', 'e'],
  ['delete', 'd'], ['add', 'a'], ['toggle', 't'], ['next_status', ']'], ['prev_status', '['],
  ['map', 'm'], ['match_all', 'M'], ['apply', 'A'],
  ['player_pause', 'space'], ['player_back', 'left'], ['player_forward', 'right'],
  ['player_next', 'n'], ['player_previous', 'N'], ['player_stop', 'X'],
  ['player_subtitle', 'v'], ['player_audio', 'b'],
];

/** Pfeiltasten funktionieren immer, auch wenn die Vim-Tasten umgelegt wurden. */
const ALIASES: [string, Action][] = [
  ['down', 'down'], ['up', 'up'], ['home', 'top'], ['end', 'bottom'],
  ['pgdn', 'page_down'], ['pgup', 'page_up'], ['=', 'increment'],
];

export interface KeyEvent {
  /** Kanonischer Name wie `enter`, `ctrl-d` oder `G`. Leer bei eingefügtem Text. */
  name: string;
  /** Eingegebener Text für Formularfelder. */
  text: string;
}

/** Die Felder von Inks `Key`, die für die Benennung zählen. */
export interface InkKey {
  upArrow: boolean; downArrow: boolean; leftArrow: boolean; rightArrow: boolean;
  pageDown: boolean; pageUp: boolean; home: boolean; end: boolean;
  return: boolean; escape: boolean; ctrl: boolean; shift: boolean; tab: boolean;
  backspace: boolean; delete: boolean; meta: boolean;
}

export function toKeyEvent(input: string, key: InkKey): KeyEvent {
  const named = (name: string): KeyEvent => ({ name, text: '' });
  if (key.return) return named('enter');
  if (key.escape) return named('esc');
  if (key.tab) return named(key.shift ? 'backtab' : 'tab');
  if (key.upArrow) return named('up');
  if (key.downArrow) return named('down');
  if (key.leftArrow) return named('left');
  if (key.rightArrow) return named('right');
  if (key.pageUp) return named('pgup');
  if (key.pageDown) return named('pgdn');
  if (key.home) return named('home');
  if (key.end) return named('end');
  if (key.backspace || key.delete) return named('backspace');
  if (key.ctrl) return named(`ctrl-${input}`);
  if (key.meta) return named(`alt-${input}`);
  if (input === ' ') return { name: 'space', text: ' ' };
  if ([...input].length === 1) return { name: input, text: input };
  return { name: '', text: input };
}

export class Keymap {
  private readonly bindings = new Map<string, Action>();
  private readonly labels = new Map<Action, string>();

  constructor(overrides: Record<string, string> = {}) {
    for (const [name, action] of ALIASES) this.bindings.set(name, action);
    for (const [action, fallback] of DEFAULTS) {
      const key = overrides[action] ?? fallback;
      this.bindings.set(key, action);
      this.labels.set(action, key);
    }
  }

  action(name: string): Action | undefined {
    return this.bindings.get(name);
  }

  label(action: Action): string {
    return this.labels.get(action) ?? '';
  }
}
