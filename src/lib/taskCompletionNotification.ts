import { invoke } from '@tauri-apps/api/core';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import type { TaskItem } from '../types';

function hasNotificationApi(): boolean {
  return typeof window !== 'undefined' && typeof window.Notification !== 'undefined';
}

function buildNotificationBody(task: TaskItem): string {
  const description = task.description.trim();
  if (description) {
    return description;
  }

  const subject = task.subject.trim();
  if (subject) {
    return subject;
  }

  return '任务已执行完成';
}

export async function ensureNotificationPermission(): Promise<boolean> {
  if (!hasNotificationApi()) {
    return false;
  }

  let granted = await isPermissionGranted();
  if (granted) {
    return true;
  }

  const permission = await requestPermission();
  granted = permission === 'granted';
  return granted;
}

export async function isTaskCompletionNotificationEnabled(): Promise<boolean> {
  return invoke<boolean>('get_task_completion_notifications_enabled');
}

export async function sendTaskCompletionNotification(task: TaskItem): Promise<boolean> {
  if (!hasNotificationApi()) {
    return false;
  }

  const enabled = await isTaskCompletionNotificationEnabled();
  if (!enabled) {
    return false;
  }

  const granted = await isPermissionGranted();
  if (!granted) {
    return false;
  }

  sendNotification({
    title: 'Aite 任务已完成',
    body: buildNotificationBody(task),
  });
  return true;
}
