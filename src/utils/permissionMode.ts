/**
 * 权限模式工具 - 用于权限模式的显示和处理
 */

import type { PermissionMode } from '../types';

/**
 * Claude Code 支持的 4 种权限模式
 */
export const PERMISSION_MODES: PermissionMode[] = [
  'default',
  'acceptEdits',
  'bypassPermissions',
  'plan',
];

/**
 * 获取权限模式的显示名称
 */
export function getPermissionModeDisplayName(mode: PermissionMode): string {
  const displayNames: Record<PermissionMode, string> = {
    'default': '默认模式',
    'acceptEdits': '自动编辑',
    'bypassPermissions': '完全自动',
    'plan': '计划模式',
  };
  return displayNames[mode] || mode;
}

/**
 * 获取权限模式的简短显示名称
 */
export function getPermissionModeShortName(mode: PermissionMode): string {
  const shortNames: Record<PermissionMode, string> = {
    'default': '默认',
    'acceptEdits': '自动',
    'bypassPermissions': '完全',
    'plan': '计划',
  };
  return shortNames[mode] || mode;
}

/**
 * 获取权限模式的描述
 */
export function getPermissionModeDescription(mode: PermissionMode): string {
  const descriptions: Record<PermissionMode, string> = {
    'default': '所有操作都需要确认',
    'acceptEdits': '自动批准文件编辑类操作',
    'bypassPermissions': '跳过所有权限确认',
    'plan': '仅生成计划，不执行操作',
  };
  return descriptions[mode] || '';
}

/**
 * 获取权限模式的图标
 */
export function getPermissionModeIcon(mode: PermissionMode): string {
  const icons: Record<PermissionMode, string> = {
    'default': '🔒',
    'acceptEdits': '✏️',
    'bypassPermissions': '⚡',
    'plan': '📋',
  };
  return icons[mode] || '🔒';
}
