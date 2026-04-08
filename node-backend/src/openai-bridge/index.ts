// OpenAI Bridge — main entry point
// Translates Anthropic Messages API → OpenAI Chat Completions / Responses API

export { createBridgeHandler, getProxyForUrl } from './handler.js';
export type { BridgeConfig, UpstreamConfig } from './types/bridge.js';
