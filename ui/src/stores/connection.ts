import { create } from "zustand";
import type { ActionBinding } from "@/types/widget";
import { useModuleStateStore } from "./moduleState";

interface ConnectionState {
	status: "connected" | "disconnected" | "connecting";
	ws: WebSocket | null;

	connect: (url: string) => void;
	disconnect: () => void;

	sendAction: (action: ActionBinding) => void;
}

export const useConnectionStore = create<ConnectionState>((set, get) => ({
	status: "disconnected",
	ws: null,
	connect: (url: string) => {
		if (get().ws) {
			get().ws?.close();
		}

		const ws = new WebSocket(url);

		ws.onopen = () => {
			set({ status: "connected", ws });
		};

		ws.onclose = () => {
			set({ status: "disconnected", ws: null });
		};

		ws.onmessage = (event) => {
			const msg = JSON.parse(event.data);
			useModuleStateStore
				.getState()
				.handleEvent(msg.module, msg.event, msg.data);
		};

		set({ status: "connecting", ws });
	},
	disconnect: () => {
		get().ws?.close();
		set({ status: "disconnected", ws: null });
	},
	sendAction: (action: ActionBinding) => {
		const ws = get().ws;
		if (ws && ws.readyState === WebSocket.OPEN) {
			const message = JSON.stringify(action);
			console.log("Sending action:", message);
			ws.send(message);
		} else {
			console.warn("WebSocket is not connected. Cannot send action.");
		}
	},
}));
