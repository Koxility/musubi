// Gemeinsame Bausteine der Ansichten.

import type { ReactNode } from 'react';
import { Box, Text } from 'ink';
import { visibleWindow } from '../text';

export const ACCENT = '#3db4f2';
export const MUTED = 'gray';
export const SELECTED = '#1f2b3d';
/** Unter dieser Breite entfällt die Detailspalte. */
export const WIDE_LAYOUT = 96;

export function Panel(props: {
  title: string;
  children: ReactNode;
  width?: number;
  height?: number;
  flexGrow?: number;
  flexBasis?: number;
}) {
  const { title, children, ...size } = props;
  return (
    <Box flexDirection="column" borderStyle="round" borderColor={MUTED} paddingX={1} {...size}>
      <Text bold color={ACCENT} wrap="truncate-end">{title}</Text>
      {children}
    </Box>
  );
}

export function Window<T>(props: {
  items: T[];
  selected: number;
  height: number;
  render: (item: T, selected: boolean, index: number) => ReactNode;
}) {
  const [start, end] = visibleWindow(props.selected, props.items.length, props.height);
  return (
    <Box flexDirection="column">
      {props.items.slice(start, end).map((item, offset) =>
        props.render(item, start + offset === props.selected, start + offset))}
    </Box>
  );
}

export type SectionRow<T> = { header: string } | { row: T; index: number };

/** Fügt vor jedem neuen Abschnitt eine Überschrift ein. */
export function withHeaders<T>(rows: T[], section: (row: T) => string): SectionRow<T>[] {
  const out: SectionRow<T>[] = [];
  let current: string | null = null;
  rows.forEach((row, index) => {
    const name = section(row);
    if (name !== current) {
      out.push({ header: name });
      current = name;
    }
    out.push({ row, index });
  });
  return out;
}

export function SectionWindow<T>(props: {
  rows: T[];
  section: (row: T) => string;
  selected: number;
  height: number;
  render: (row: T, selected: boolean) => ReactNode;
}) {
  const lines = withHeaders(props.rows, props.section);
  const display = Math.max(lines.findIndex((l) => 'row' in l && l.index === props.selected), 0);
  return (
    <Window
      items={lines}
      selected={display}
      height={props.height}
      render={(line, _selected, index) =>
        'header' in line
          ? <Text key={`h-${index}`} bold>{line.header}</Text>
          : <Box key={index}>{props.render(line.row, line.index === props.selected)}</Box>}
    />
  );
}

export function Empty({ text }: { text: string }) {
  return <Text color={MUTED}>{text}</Text>;
}
