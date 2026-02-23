import { create } from "zustand";
import type { ActionBinding } from "@/types/widget";
import { useModuleStateStore } from "./moduleState";

interface CallMessage {
  type: "call";
  request_id: string;
  module: string;
  name: string;
  version: number;
  params: Record<string, unknown>;
}

interface ConnectionState {
  status: "connected" | "disconnected" | "connecting";
  ws: WebSocket | null;
  connect: (url: string) => void;
  disconnect: () => void;
  sendAction: (action: ActionBinding) => void;
}

function generateId(): string {
  return Math.random().toString(36).slice(2, 10);
}

export const useConnectionStore = create<ConnectionState>((set, get) => ({
  status: "disconnected",
  ws: null,

  connect: (url: string) => {
    get().ws?.close();
    const ws = new WebSocket(url);

    ws.onopen = () => set({ status: "connected", ws });
    ws.onclose = () => set({ status: "disconnected", ws: null });

    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      if (msg.type === "event") {
        // New format: { type, module, name, version, data, timestamp }
        useModuleStateStore.getState().handleEvent(msg.module, msg.name, msg.data);
      }
      // type === "response" is ignored for now (no pending request tracking yet)
    };

    set({ status: "connecting", ws });
  },

  disconnect: () => {
    get().ws?.close();
    set({ status: "disconnected", ws: null });
  },

  sendAction: (action: ActionBinding) => {
    const ws = get().ws;
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      console.warn("WebSocket not connected");
      return;
    }
    const msg: CallMessage = {
      type: "call",
      request_id: generateId(),
      module: action.module,
      name: action.action,   // ActionBinding uses "action" field, wire protocol uses "name"
      version: 1,
      params: action.params ?? {},
    };
    ws.send(JSON.stringify(msg));
  },
}));
