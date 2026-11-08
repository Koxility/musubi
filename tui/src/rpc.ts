// Verbindung zum Rust-Kern: eine JSON-Zeile pro Nachricht über einen Unix-Socket.

import net from 'node:net';
import type { CoreEvent } from './types';

export interface Api {
  call<T>(method: string, params?: unknown): Promise<T>;
}

interface Pending {
  resolve: (value: never) => void;
  reject: (error: Error) => void;
}

export class RpcClient implements Api {
  private nextId = 1;
  private buffer = '';
  private readonly pending = new Map<number, Pending>();
  onEvent: (event: CoreEvent) => void = () => {};
  onClose: () => void = () => {};

  private constructor(private readonly socket: net.Socket) {
    socket.setEncoding('utf8');
    socket.on('data', (chunk: string) => this.receive(chunk));
    socket.on('close', () => {
      for (const { reject } of this.pending.values()) reject(new Error('connection to musubi closed'));
      this.pending.clear();
      this.onClose();
    });
  }

  static connect(path: string): Promise<RpcClient> {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection(path);
      socket.once('error', reject);
      socket.once('connect', () => {
        socket.removeListener('error', reject);
        socket.on('error', () => socket.destroy());
        resolve(new RpcClient(socket));
      });
    });
  }

  call<T>(method: string, params: unknown = {}): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: never) => void, reject });
      this.socket.write(`${JSON.stringify({ id, method, params })}\n`);
    });
  }

  close(): void {
    this.socket.end();
  }

  private receive(chunk: string): void {
    this.buffer += chunk;
    let newline: number;
    while ((newline = this.buffer.indexOf('\n')) >= 0) {
      const line = this.buffer.slice(0, newline);
      this.buffer = this.buffer.slice(newline + 1);
      this.handle(line);
    }
  }

  private handle(line: string): void {
    const message = JSON.parse(line) as {
      id?: number;
      result?: never;
      error?: string;
      event?: CoreEvent;
    };
    if (message.event) {
      this.onEvent(message.event);
      return;
    }
    const waiting = message.id === undefined ? undefined : this.pending.get(message.id);
    if (!waiting || message.id === undefined) return;
    this.pending.delete(message.id);
    if (message.error !== undefined) waiting.reject(new Error(message.error));
    else waiting.resolve(message.result as never);
  }
}
