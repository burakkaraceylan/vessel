/** biome-ignore-all lint/suspicious/noExplicitAny: we need any for unknown params */
export type IncomingMessage = {
	module: string;
	action: string;
	params: Record<string, any>;
};

export type OutgoingMessage = {
	module: string;
	event: string;
	data: Record<string, any>;
};
