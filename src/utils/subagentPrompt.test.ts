import test from 'node:test';
import assert from 'node:assert/strict';

import type { FeedEntry, Message } from '../types';
import { extractMessagePlainText, getLeadingSubagentPrompt } from './subagentPrompt';

function makeMessage(overrides: Partial<Message>): Message {
  return {
    id: overrides.id || 'msg',
    role: overrides.role || 'assistant',
    content: overrides.content || '',
    timestamp: overrides.timestamp || Date.now(),
    ...overrides,
  };
}

test('extractMessagePlainText reads plain text from message content', () => {
  const message = makeMessage({
    role: 'user',
    content: '请处理这个项目',
  });

  assert.equal(extractMessagePlainText(message), '请处理这个项目');
});

test('extractMessagePlainText reads text blocks from json content', () => {
  const message = makeMessage({
    role: 'user',
    content: JSON.stringify([
      { type: 'text', text: '第一行' },
      { type: 'text', content: '第二行' },
      { type: 'tool_use', name: 'Bash' },
    ]),
  });

  assert.equal(extractMessagePlainText(message), '第一行\n第二行');
});

test('getLeadingSubagentPrompt hides only the leading subagent user prompt', () => {
  const promptEntry: FeedEntry = {
    kind: 'message',
    msg: makeMessage({
      id: 'prompt-msg',
      role: 'user',
      content: '请分析这个仓库的架构',
    }),
  };

  const assistantEntry: FeedEntry = {
    kind: 'message',
    msg: makeMessage({
      id: 'assistant-msg',
      role: 'assistant',
      content: '我来分析这个仓库。',
    }),
  };

  const laterUserEntry: FeedEntry = {
    kind: 'message',
    msg: makeMessage({
      id: 'later-user-msg',
      role: 'user',
      content: '补充一点：优先看网关模块',
    }),
  };

  const info = getLeadingSubagentPrompt([promptEntry, assistantEntry, laterUserEntry]);

  assert.equal(info.promptText, '请分析这个仓库的架构');
  assert.equal(info.hiddenMessageIds.has('prompt-msg'), true);
  assert.equal(info.hiddenMessageIds.has('later-user-msg'), false);
});
