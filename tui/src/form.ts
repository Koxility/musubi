// Allgemeines Formular für Login, Eintragsbearbeitung, Filter und Zuordnung.

import type { KeyEvent } from './keys';

export type FieldKind = 'text' | 'secret' | 'choice' | 'toggle';

export interface Field {
  key: string;
  label: string;
  kind: FieldKind;
  value: string;
  options?: string[];
}

export const field = {
  text: (key: string, label: string, value = ''): Field => ({ key, label, kind: 'text', value }),
  secret: (key: string, label: string): Field => ({ key, label, kind: 'secret', value: '' }),
  choice: (key: string, label: string, options: string[], value: string): Field => ({
    key, label, kind: 'choice', options, value,
  }),
  toggle: (key: string, label: string, value: boolean): Field => ({
    key, label, kind: 'toggle', value: value ? 'yes' : 'no',
  }),
};

export type FormOutcome = 'editing' | 'submit' | 'cancel';

export class Form {
  focus = 0;
  error: string | null = null;

  constructor(
    public title: string,
    public fields: Field[],
    public hint: string | null = null,
  ) {}

  value(key: string): string {
    return this.fields.find((f) => f.key === key)?.value ?? '';
  }

  has(key: string): boolean {
    return this.fields.some((f) => f.key === key);
  }

  isOn(key: string): boolean {
    return this.value(key) === 'yes';
  }

  private cycle(target: Field, forward: boolean): void {
    if (target.kind === 'toggle') {
      target.value = target.value === 'yes' ? 'no' : 'yes';
    } else if (target.kind === 'choice' && target.options?.length) {
      const count = target.options.length;
      const current = Math.max(0, target.options.indexOf(target.value));
      target.value = target.options[(current + (forward ? 1 : count - 1)) % count]!;
    }
  }

  private move(forward: boolean): void {
    const count = this.fields.length;
    if (count > 0) this.focus = (this.focus + (forward ? 1 : count - 1)) % count;
  }

  onKey(key: KeyEvent): FormOutcome {
    switch (key.name) {
      case 'esc':
        return 'cancel';
      case 'enter':
        return 'submit';
      case 'tab':
      case 'down':
        this.move(true);
        return 'editing';
      case 'backtab':
      case 'up':
        this.move(false);
        return 'editing';
    }
    const target = this.fields[this.focus];
    if (!target) return 'editing';
    const editable = target.kind === 'text' || target.kind === 'secret';
    if (editable) {
      if (key.name === 'backspace') target.value = [...target.value].slice(0, -1).join('');
      else if (key.name === 'ctrl-u') target.value = '';
      else if (key.text) target.value += key.text;
    } else if (key.name === 'right' || key.name === 'space') {
      this.cycle(target, true);
    } else if (key.name === 'left') {
      this.cycle(target, false);
    }
    return 'editing';
  }
}

export function displayValue(target: Field): string {
  switch (target.kind) {
    case 'secret':
      return '•'.repeat([...target.value].length);
    case 'choice':
      return `‹ ${target.value} ›`;
    case 'toggle':
      return `[${target.value === 'yes' ? 'x' : ' '}]`;
    case 'text':
      return target.value;
  }
}
