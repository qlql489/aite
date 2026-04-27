import test from 'node:test';
import assert from 'node:assert/strict';

import type { Message, SubagentRuntimeState } from '../types';
import { groupMessages } from './messageGrouping';

function makeMessage(overrides: Partial<Message>): Message {
  return {
    id: overrides.id || 'msg',
    role: overrides.role || 'assistant',
    content: overrides.content || '',
    timestamp: overrides.timestamp || Date.now(),
    ...overrides,
  };
}

test('groupMessages creates subagent group from runtime before child messages arrive', () => {
  const messages: Message[] = [
    makeMessage({
      id: 'assistant-task',
      contentBlocks: [
        {
          type: 'tool_use',
          id: 'task-1',
          name: 'Task',
          input: {
            description: '扫描代码库',
            subagent_type: 'Explore',
          },
        },
      ],
    }),
  ];

  const runtime = new Map<string, SubagentRuntimeState>([
    ['task-1', {
      taskToolUseId: 'task-1',
      description: '扫描代码库',
      agentType: 'Explore',
      status: 'running',
      startedAt: 1000,
      toolCallCount: 1,
      latestPreview: 'Read src/main.ts',
      calls: [
        {
          id: 'call-1',
          name: 'Read',
          input: { file_path: 'src/main.ts' },
          status: 'running',
          startedAt: 1000,
          updatedAt: 1000,
        },
      ],
    }],
  ]);

  const entries = groupMessages(messages, runtime);

  assert.equal(entries.length, 1);
  assert.equal(entries[0].kind, 'tool_msg_group');
  if (entries[0].kind !== 'tool_msg_group') {
    throw new Error('expected task group');
  }
  assert.equal(entries[0].subagentGroups?.length, 1);
  const subagent = entries[0].subagentGroups?.[0];
  assert.ok(subagent);
  assert.equal(subagent.liveCalls?.length, 1);
  assert.equal(subagent.children.length, 0);
});

test('groupMessages keeps live trace and dedupes child tool groups for same runtime call', () => {
  const messages: Message[] = [
    makeMessage({
      id: 'assistant-task',
      contentBlocks: [
        {
          type: 'tool_use',
          id: 'task-1',
          name: 'Task',
          input: {
            description: '扫描代码库',
            subagent_type: 'Explore',
          },
        },
      ],
    }),
    makeMessage({
      id: 'assistant-subagent-tool',
      parentToolUseId: 'task-1',
      contentBlocks: [
        {
          type: 'tool_use',
          id: 'call-1',
          name: 'Read',
          input: { file_path: 'src/main.ts' },
        },
      ],
    }),
  ];

  const runtime = new Map<string, SubagentRuntimeState>([
    ['task-1', {
      taskToolUseId: 'task-1',
      description: '扫描代码库',
      agentType: 'Explore',
      status: 'running',
      startedAt: 1000,
      toolCallCount: 1,
      latestPreview: 'Read src/main.ts',
      calls: [
        {
          id: 'call-1',
          name: 'Read',
          input: { file_path: 'src/main.ts' },
          status: 'running',
          startedAt: 1000,
          updatedAt: 1000,
        },
      ],
    }],
  ]);

  const entries = groupMessages(messages, runtime);

  assert.equal(entries.length, 1);
  assert.equal(entries[0].kind, 'tool_msg_group');
  if (entries[0].kind !== 'tool_msg_group') {
    throw new Error('expected task group');
  }
  assert.equal(entries[0].taskRuntimeSummary?.toolCallCount, 1);
  assert.equal(entries[0].subagentGroups?.length, 1);
  const subagent = entries[0].subagentGroups?.[0];
  assert.ok(subagent);
  assert.equal(subagent.liveCalls?.length, 1);
  assert.equal(subagent.children.length, 0);
});

test('groupMessages nests child messages under Agent tool_use encoded as string content', () => {
  const messages: Message[] = [
    makeMessage({
      id: 'assistant-agent-tool',
      contentBlocks: [
        {
          type: 'tool_use',
          content: JSON.stringify({
            id: 'agent-call-1',
            name: 'Agent',
            input: {
              description: '探索 openclaw 架构模式',
              subagent_type: 'Explore',
            },
          }),
        } as any,
      ],
    }),
    makeMessage({
      id: 'assistant-subagent-tool',
      parent_tool_use_id: 'agent-call-1',
      contentBlocks: [
        {
          type: 'tool_use',
          content: JSON.stringify({
            id: 'call-1',
            name: 'Bash',
            input: { command: 'ls -la /tmp/project' },
          }),
        } as any,
      ],
    }),
  ];

  const entries = groupMessages(messages, new Map());

  assert.equal(entries.length, 1);
  assert.equal(entries[0].kind, 'tool_msg_group');
  if (entries[0].kind !== 'tool_msg_group') {
    throw new Error('expected agent group');
  }
  assert.equal(entries[0].toolName, 'Agent');
  assert.equal(entries[0].subagentGroups?.length, 1);
  const subagent = entries[0].subagentGroups?.[0];
  assert.ok(subagent);
  assert.equal(subagent.children.length, 1);
  assert.equal(subagent.children[0].kind, 'tool_msg_group');
});

test('groupMessages treats subagent final assistant result as completed despite intermediate tool errors', () => {
  const messages: Message[] = [
    makeMessage({
      id: 'assistant-agent-tool',
      contentBlocks: [
        {
          type: 'tool_use',
          id: 'agent-call-1',
          name: 'Agent',
          input: {
            description: '探索 openclaw 架构模式',
            subagent_type: 'Explore',
          },
        },
      ],
    }),
    makeMessage({
      id: 'assistant-final',
      role: 'assistant',
      parentToolUseId: 'agent-call-1',
      content: '最终架构分析报告',
      contentBlocks: [
        {
          type: 'text',
          text: '最终架构分析报告',
        },
      ],
    }),
  ];

  const runtime = new Map<string, SubagentRuntimeState>([
    ['agent-call-1', {
      taskToolUseId: 'agent-call-1',
      description: '探索 openclaw 架构模式',
      agentType: 'Explore',
      status: 'error',
      startedAt: 1000,
      completedAt: 2000,
      toolCallCount: 1,
      latestPreview: 'Read README.md',
      calls: [
        {
          id: 'call-1',
          name: 'Read',
          input: { file_path: 'README.md' },
          status: 'error',
          isError: true,
          startedAt: 1000,
          updatedAt: 1500,
          completedAt: 1500,
        },
      ],
    }],
  ]);

  const entries = groupMessages(messages, runtime);

  assert.equal(entries.length, 1);
  assert.equal(entries[0].kind, 'tool_msg_group');
  if (entries[0].kind !== 'tool_msg_group') {
    throw new Error('expected agent group');
  }

  assert.equal(entries[0].taskRuntimeSummary?.status, 'completed');
  const subagent = entries[0].subagentGroups?.[0];
  assert.ok(subagent);
  assert.equal(subagent.status, 'completed');
});
