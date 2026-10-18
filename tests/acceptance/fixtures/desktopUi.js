// Only the operating-system boundary is synthetic; App, routing, stores and IPC dispatch are production.
import { mount } from "svelte";
import { mockIPC } from "@tauri-apps/api/mocks";
import App from "../../../apps/dav-bridge-desktop/src/App.svelte";
mockIPC((command, payload) => window.uiIpc({ command, payload }));
mount(App, { target: document.body });
