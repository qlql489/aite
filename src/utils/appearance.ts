export const DEFAULT_INTERFACE_FONT_SIZE = 16;
export const MIN_INTERFACE_FONT_SIZE = 14;
export const MAX_INTERFACE_FONT_SIZE = 20;

export const DEFAULT_CHAT_FONT_SIZE = 14;
export const MIN_CHAT_FONT_SIZE = 12;
export const MAX_CHAT_FONT_SIZE = 24;

export const INTERFACE_FONT_SIZE_EVENT = 'aite:interface-font-size-change';
export const CHAT_FONT_SIZE_EVENT = 'aite:chat-font-size-change';

export interface AppearanceConfig {
  interfaceFontSize?: number;
  chatFontSize?: number;
}

function clampSize(size: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(size)) return fallback;
  return Math.min(max, Math.max(min, Math.round(size)));
}

export function clampInterfaceFontSize(size: number): number {
  return clampSize(size, MIN_INTERFACE_FONT_SIZE, MAX_INTERFACE_FONT_SIZE, DEFAULT_INTERFACE_FONT_SIZE);
}

export function clampChatFontSize(size: number): number {
  return clampSize(size, MIN_CHAT_FONT_SIZE, MAX_CHAT_FONT_SIZE, DEFAULT_CHAT_FONT_SIZE);
}

export function applyInterfaceFontSize(size: number): number {
  const nextSize = clampInterfaceFontSize(size);
  const root = document.documentElement;
  root.style.setProperty('--interface-font-size-px', `${nextSize}px`);
  root.style.fontSize = `${nextSize}px`;
  window.dispatchEvent(new CustomEvent<number>(INTERFACE_FONT_SIZE_EVENT, { detail: nextSize }));
  return nextSize;
}

export function applyChatFontSize(size: number): number {
  const nextSize = clampChatFontSize(size);
  document.documentElement.style.setProperty('--chat-font-size-px', `${nextSize}px`);
  window.dispatchEvent(new CustomEvent<number>(CHAT_FONT_SIZE_EVENT, { detail: nextSize }));
  return nextSize;
}

export function applyAppearanceConfig(config: AppearanceConfig): AppearanceConfig {
  return {
    interfaceFontSize: applyInterfaceFontSize(config.interfaceFontSize ?? DEFAULT_INTERFACE_FONT_SIZE),
    chatFontSize: applyChatFontSize(config.chatFontSize ?? DEFAULT_CHAT_FONT_SIZE),
  };
}
