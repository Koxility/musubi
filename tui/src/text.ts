// Reine Hilfen für Anzeige und Filter.

export function formatTime(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(h > 0 ? 2 : 1, '0');
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** Zeit bis zu einem Ereignis, z. B. `2d 3h` oder `12m`. */
export function countdown(seconds: number): string {
  if (seconds <= 0) return 'airing';
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${Math.max(minutes, 1)}m`;
}

export function progressBar(fraction: number, width: number): string {
  const filled = Math.round(Math.min(1, Math.max(0, fraction)) * width);
  return '█'.repeat(filled) + '░'.repeat(Math.max(0, width - filled));
}

/** Sichtbarer Ausschnitt einer Liste, die Auswahl bleibt möglichst mittig. */
export function visibleWindow(selected: number, count: number, height: number): [number, number] {
  if (count <= height) return [0, count];
  const start = Math.max(0, Math.min(selected - Math.floor(height / 2), count - height));
  return [start, start + height];
}

function fuzzyScore(text: string, query: string): number | null {
  const caseSensitive = query !== query.toLowerCase();
  const haystack = caseSensitive ? text : text.toLowerCase();
  let score = 0;
  let position = -1;
  for (const char of query) {
    const found = haystack.indexOf(char, position + 1);
    if (found < 0) return null;
    score += found === position + 1 ? 10 : 1;
    if (found === 0 || text[found - 1] === ' ') score += 6;
    position = found;
  }
  return score;
}

/** Indizes der passenden Texte, die besten Treffer zuerst. Leeres Muster behält alles. */
export function fuzzyFilter(texts: string[], query: string): number[] {
  if (query === '') return texts.map((_, index) => index);
  return texts
    .flatMap((text, index) => {
      const score = fuzzyScore(text, query);
      return score === null ? [] : [{ index, score }];
    })
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((match) => match.index);
}
