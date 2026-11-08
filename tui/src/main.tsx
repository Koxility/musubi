// Einstiegspunkt des Ink-Frontends. `musubi tui` startet dieses Programm.

import { render } from 'ink';
import { RpcClient } from './rpc';
import { Store } from './store';
import { App } from './ui/App';

const socket = process.env.MUSUBI_RPC_SOCKET;
if (!socket) {
  process.stderr.write('Start this program through `musubi tui`.\n');
  process.exit(2);
}

const client = await RpcClient.connect(socket);
const store = new Store(client);
client.onEvent = (event) => store.onCoreEvent(event);
client.onClose = () => {
  store.shouldQuit = true;
  store.emit();
};

const app = render(<App store={store} />, { exitOnCtrlC: false, alternateScreen: true, maxFps: 30 });
const ticker = setInterval(() => store.onTick(), 1000);
store.start().catch((error: unknown) => {
  store.setNotice(false, error instanceof Error ? error.message : String(error));
  store.emit();
});

await app.waitUntilExit();
clearInterval(ticker);
client.close();
process.exit(0);
