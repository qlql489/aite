<script setup lang="ts">
import { computed, ref, watch, onMounted, nextTick } from 'vue';
import { marked, Renderer, type MarkedExtension } from 'marked';
import DOMPurify from 'dompurify';
import hljs from 'highlight.js';
import 'highlight.js/styles/github.css';
import { escapeRegExp } from '../../utils/sessionSearch';

// 注册常用语言
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import go from 'highlight.js/lib/languages/go';
import java from 'highlight.js/lib/languages/java';
import cpp from 'highlight.js/lib/languages/cpp';
import c from 'highlight.js/lib/languages/c';
import bash from 'highlight.js/lib/languages/bash';
import shell from 'highlight.js/lib/languages/shell';
import css from 'highlight.js/lib/languages/css';
import scss from 'highlight.js/lib/languages/scss';
import html from 'highlight.js/lib/languages/xml';
import json from 'highlight.js/lib/languages/json';
import yaml from 'highlight.js/lib/languages/yaml';
import markdown from 'highlight.js/lib/languages/markdown';
import sql from 'highlight.js/lib/languages/sql';
import php from 'highlight.js/lib/languages/php';
import ruby from 'highlight.js/lib/languages/ruby';
import swift from 'highlight.js/lib/languages/swift';
import kotlin from 'highlight.js/lib/languages/kotlin';
import dart from 'highlight.js/lib/languages/dart';
import vue from 'highlight.js/lib/languages/xml';
import xml from 'highlight.js/lib/languages/xml';
import dockerfile from 'highlight.js/lib/languages/dockerfile';
import nginx from 'highlight.js/lib/languages/nginx';
import plaintext from 'highlight.js/lib/languages/plaintext';

// 注册语言
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('js', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('ts', typescript);
hljs.registerLanguage('python', python);
hljs.registerLanguage('py', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('rs', rust);
hljs.registerLanguage('go', go);
hljs.registerLanguage('java', java);
hljs.registerLanguage('cpp', cpp);
hljs.registerLanguage('c++', cpp);
hljs.registerLanguage('c', c);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('sh', shell);
hljs.registerLanguage('shell', shell);
hljs.registerLanguage('css', css);
hljs.registerLanguage('scss', scss);
hljs.registerLanguage('html', html);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('json', json);
hljs.registerLanguage('yaml', yaml);
hljs.registerLanguage('yml', yaml);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('md', markdown);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('php', php);
hljs.registerLanguage('ruby', ruby);
hljs.registerLanguage('swift', swift);
hljs.registerLanguage('kotlin', kotlin);
hljs.registerLanguage('dart', dart);
hljs.registerLanguage('vue', vue);
hljs.registerLanguage('dockerfile', dockerfile);
hljs.registerLanguage('docker', dockerfile);
hljs.registerLanguage('nginx', nginx);
hljs.registerLanguage('plaintext', plaintext);
hljs.registerLanguage('text', plaintext);

interface Props {
  content: string;
  isStreaming?: boolean;
  searchQuery?: string;
  activeSearchMatch?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  isStreaming: false,
  searchQuery: '',
  activeSearchMatch: false,
});

const containerRef = ref<HTMLElement | null>(null);
const STREAMING_CURSOR_MARKUP = '<span class="cursor" aria-hidden="true"></span>';

const hasUnclosedFencedCodeBlock = (markdownText: string): boolean => {
  const fencePattern = /^ {0,3}(`{3,}|~{3,}).*$/gm;
  let openFence: { marker: string; length: number } | null = null;

  for (const match of markdownText.matchAll(fencePattern)) {
    const fence = match[1];
    const marker = fence[0];
    const length = fence.length;

    if (!openFence) {
      openFence = { marker, length };
      continue;
    }

    if (openFence.marker === marker && length >= openFence.length) {
      openFence = null;
    }
  }

  return openFence !== null;
};

const injectStreamingCursor = (rawHtml: string, markdownText: string): string => {
  if (!props.isStreaming) return rawHtml;

  if (hasUnclosedFencedCodeBlock(markdownText)) {
    const codeBlockEndMarker = '</code></pre>';
    const lastCodeBlockEndIndex = rawHtml.lastIndexOf(codeBlockEndMarker);

    if (lastCodeBlockEndIndex >= 0) {
      return `${rawHtml.slice(0, lastCodeBlockEndIndex)}${STREAMING_CURSOR_MARKUP}${rawHtml.slice(lastCodeBlockEndIndex)}`;
    }
  }

  return `${rawHtml}${STREAMING_CURSOR_MARKUP}`;
};

// 配置 marked 使用自定义 renderer 来支持代码高亮
const renderer = new Renderer();
renderer.code = function(code) {
  const language = code.lang || 'plaintext';

  let highlighted: string;
  let detectedLang = language;

  // 尝试使用指定语言高亮，如果失败则自动检测
  if (language && hljs.getLanguage(language)) {
    const result = hljs.highlight(code.text, { language });
    highlighted = result.value;
  } else {
    // 自动检测语言
    const result = hljs.highlightAuto(code.text);
    highlighted = result.value;
    detectedLang = result.language || language;
  }

  // 生成唯一 ID
  const blockId = `code-block-${Math.random().toString(36).substr(2, 9)}`;

  // 创建带标题栏的代码块 HTML
  return `
    <div class="enhanced-code-block" data-block-id="${blockId}" data-language="${language}">
      <div class="code-header">
        <div class="code-header-left">
          <span class="language-badge">${(detectedLang || language).toUpperCase()}</span>
        </div>
        <div class="code-header-right">
          <button class="copy-btn" data-action="copy" title="Copy code">
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
            </svg>
            <span>Copy</span>
          </button>
          <button class="copy-btn" data-action="copy-md" title="Copy as Markdown">
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="16 18 22 12 16 6"></polyline>
              <polyline points="8 6 2 12 8 18"></polyline>
            </svg>
            <span>Markdown</span>
          </button>
        </div>
      </div>
      <pre class="code-pre"><code class="hljs language-${detectedLang}">${highlighted}</code></pre>
    </div>
  `;
};

marked.use({ renderer } as MarkedExtension);

const renderedHtml = computed(() => {
  if (!props.content) return '';

  try {
    const rawHtml = marked.parse(props.content) as string;
    const htmlWithCursor = injectStreamingCursor(rawHtml, props.content);
    // 清理 HTML，防止 XSS
    return DOMPurify.sanitize(htmlWithCursor);
  } catch (e) {
    return DOMPurify.sanitize(
      props.isStreaming
        ? `${props.content}${STREAMING_CURSOR_MARKUP}`
        : props.content,
    );
  }
});

// 处理代码块交互
const setupCodeBlockInteractions = () => {
  if (!containerRef.value) return;

  // 处理复制按钮
  const copyButtons = containerRef.value.querySelectorAll('.copy-btn');
  copyButtons.forEach(btn => {
    btn.addEventListener('click', handleCopyClick);
  });
};

const handleCopyClick = (e: Event) => {
  const target = e.currentTarget as HTMLElement;
  const action = target.getAttribute('data-action');
  const codeBlock = target.closest('.enhanced-code-block');
  if (!codeBlock) return;

  const pre = codeBlock.querySelector('pre');
  if (!pre) return;

  const code = pre.textContent || '';

  if (action === 'copy') {
    copyToClipboard(code, target);
  } else if (action === 'copy-md') {
    const language = codeBlock.getAttribute('data-language') || 'text';
    const markdown = `\`\`\`${language}\n${code}\n\`\`\``;
    copyToClipboard(markdown, target);
  }
};

const clearSearchHighlights = () => {
  if (!containerRef.value) return;

  const highlights = containerRef.value.querySelectorAll('mark.session-search-hit');
  highlights.forEach((highlight) => {
    const parent = highlight.parentNode;
    if (!parent) return;

    parent.replaceChild(document.createTextNode(highlight.textContent || ''), highlight);
    parent.normalize();
  });
};

const applySearchHighlights = () => {
  if (!containerRef.value) return;

  clearSearchHighlights();

  const query = props.searchQuery.trim();
  if (!query) return;

  const matcher = new RegExp(escapeRegExp(query), 'gi');
  const textNodes: Text[] = [];
  const walker = document.createTreeWalker(
    containerRef.value,
    NodeFilter.SHOW_TEXT,
    {
      acceptNode(node) {
        const value = node.nodeValue || '';
        if (!value.trim()) {
          return NodeFilter.FILTER_REJECT;
        }

        const parent = node.parentElement;
        if (!parent) {
          return NodeFilter.FILTER_REJECT;
        }

        if (
          parent.closest('button')
          || parent.closest('svg')
          || parent.closest('.cursor')
        ) {
          return NodeFilter.FILTER_REJECT;
        }

        matcher.lastIndex = 0;
        return matcher.test(value) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
      },
    },
  );

  let currentNode = walker.nextNode();
  while (currentNode) {
    textNodes.push(currentNode as Text);
    currentNode = walker.nextNode();
  }

  textNodes.forEach((node) => {
    const value = node.nodeValue || '';
    matcher.lastIndex = 0;

    let lastIndex = 0;
    let match: RegExpExecArray | null;
    const fragment = document.createDocumentFragment();

    while ((match = matcher.exec(value)) !== null) {
      if (match.index > lastIndex) {
        fragment.appendChild(document.createTextNode(value.slice(lastIndex, match.index)));
      }

      const highlight = document.createElement('mark');
      highlight.className = props.activeSearchMatch
        ? 'session-search-hit active'
        : 'session-search-hit';
      highlight.textContent = match[0];
      fragment.appendChild(highlight);

      lastIndex = match.index + match[0].length;
      if (match[0].length === 0) {
        matcher.lastIndex += 1;
      }
    }

    if (lastIndex < value.length) {
      fragment.appendChild(document.createTextNode(value.slice(lastIndex)));
    }

    node.parentNode?.replaceChild(fragment, node);
  });
};

const copyToClipboard = async (text: string, button: HTMLElement) => {
  try {
    await navigator.clipboard.writeText(text);

    // 显示复制成功状态
    const originalContent = button.innerHTML;
    button.innerHTML = `
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="20 6 9 17 4 12"></polyline>
      </svg>
      <span>Copied</span>
    `;

    setTimeout(() => {
      button.innerHTML = originalContent;
    }, 2000);
  } catch {
    // clipboard not available
  }
};

// 监听内容变化，重新设置交互
watch(() => [props.content, props.searchQuery, props.activeSearchMatch], async () => {
  await nextTick();
  setupCodeBlockInteractions();
  applySearchHighlights();
}, { deep: true });

onMounted(() => {
  setupCodeBlockInteractions();
  applySearchHighlights();
});
</script>

<template>
  <div ref="containerRef" class="markdown-content" v-html="renderedHtml"></div>
</template>

<style scoped>
.markdown-content {
  --markdown-body-font-size: 1em;
  --markdown-inline-code-font-size: 0.92em;
  --markdown-code-font-size: 0.93em;
  --markdown-code-header-font-size: 0.79em;
  font-size: var(--markdown-body-font-size);
  line-height: 1.6;
  word-wrap: break-word;
  width: 100%;
  max-width: none;
  margin: 0;
}

.markdown-content :deep(h1),
.markdown-content :deep(h2),
.markdown-content :deep(h3),
.markdown-content :deep(h4),
.markdown-content :deep(h5),
.markdown-content :deep(h6) {
  margin-top: 1.5rem;
  margin-bottom: 0.75rem;
  font-weight: 600;
  line-height: 1.25;
}

.markdown-content :deep(h1) {
  font-size: 1.5em;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  padding-bottom: 0.375rem;
}

.markdown-content :deep(h2) {
  font-size: 1.25em;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
  padding-bottom: 0.3rem;
}

.markdown-content :deep(h3) {
  font-size: 1.125em;
}

.markdown-content :deep(p) {
  margin-bottom: 1rem;
}

.markdown-content :deep(code) {
  background-color: var(--bg-tertiary, #f3f4f6);
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
  font-family: 'Monaco', 'Menlo', monospace;
  font-size: var(--markdown-inline-code-font-size);
}

.markdown-content :deep(ul),
.markdown-content :deep(ol) {
  margin-bottom: 1rem;
  padding-left: 1.5rem;
}

.markdown-content :deep(li) {
  margin-bottom: 0.25rem;
}

.markdown-content :deep(blockquote) {
  border-left: 4px solid var(--primary-color, #3b82f6);
  padding-left: 1rem;
  margin-bottom: 1rem;
  color: var(--text-secondary, #6b7280);
}

.markdown-content :deep(a) {
  color: var(--primary-color, #3b82f6);
  text-decoration: none;
}

.markdown-content :deep(a:hover) {
  text-decoration: underline;
}

.markdown-content :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin-bottom: 1rem;
}

.markdown-content :deep(th),
.markdown-content :deep(td) {
  border: 1px solid var(--border-color, #e5e7eb);
  padding: 0.5rem;
  text-align: left;
}

.markdown-content :deep(th) {
  background-color: var(--bg-secondary, #f9fafb);
  font-weight: 600;
}

.markdown-content :deep(img) {
  max-width: 100%;
  border-radius: 0.5rem;
  margin: 0.5rem 0;
}

/* 流式光标效果 */
.markdown-content :deep(.cursor) {
  display: inline-block;
  width: 2px;
  height: 1em;
  background-color: var(--primary-color, #3b82f6);
  animation: blink 1s infinite;
  vertical-align: text-bottom;
}

.markdown-content :deep(mark.session-search-hit) {
  padding: 0.06em 0.16em;
  border-radius: 0.25rem;
  background: rgba(250, 204, 21, 0.28);
  color: inherit;
}

.markdown-content :deep(mark.session-search-hit.active) {
  background: rgba(249, 115, 22, 0.3);
  box-shadow: inset 0 0 0 1px rgba(249, 115, 22, 0.2);
}

@keyframes blink {
  0%, 50% { opacity: 1; }
  51%, 100% { opacity: 0; }
}

/* 增强的代码块样式 */
.markdown-content :deep(.enhanced-code-block) {
  position: relative;
  margin: 0.75rem 0;
  border-radius: 0.5rem;
  overflow: hidden;
  border: 1px solid #e5e7eb;
}

.markdown-content :deep(.code-header) {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.375rem 1rem;
  font-size: var(--markdown-code-header-font-size);
  background-color: #f3f4f6;
  color: #4b5563;
  border-bottom: 1px solid #e5e7eb;
}

.markdown-content :deep(.code-header-left) {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
}

.markdown-content :deep(.language-badge) {
  background-color: #e5e7eb;
  border-radius: 0.25rem;
  padding: 0.125rem 0.375rem;
  font-size: 0.95em;
  font-weight: 500;
  color: #374151;
}

.markdown-content :deep(.code-header-right) {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  margin-left: 0.5rem;
  flex-shrink: 0;
}

.markdown-content :deep(.copy-btn) {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  border-radius: 0.25rem;
  padding: 0.25rem 0.375rem;
  color: #4b5563;
  background: transparent;
  border: none;
  cursor: pointer;
  transition: all 0.2s;
  font-size: 1em;
}

.markdown-content :deep(.copy-btn:hover) {
  background-color: #e5e7eb;
  color: #1f2937;
}

.markdown-content :deep(.copy-btn svg) {
  width: 0.875rem;
  height: 0.875rem;
}

.markdown-content :deep(.code-pre) {
  margin: 0;
  padding: 0.75rem;
  overflow: auto;
  font-size: var(--markdown-code-font-size);
  line-height: 1.5;
}

.markdown-content :deep(.code-pre code) {
  font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
  background: transparent !important;
  padding: 0;
}

/* 确保 highlight.js 的样式不会被覆盖 */
.markdown-content :deep(.hljs) {
  color: inherit;
}

.markdown-content :deep(.code-pre) {
  background-color: #f6f8fa;
}

/* 深色模式 */
@media (prefers-color-scheme: dark) {
  .markdown-content :deep(h1),
  .markdown-content :deep(h2) {
    border-bottom-color: var(--border-color, #374151);
  }

  .markdown-content :deep(code) {
    background-color: var(--bg-tertiary, #374151);
  }

  .markdown-content :deep(blockquote) {
    border-left-color: var(--primary-color, #3b82f6);
  }

  .markdown-content :deep(th) {
    background-color: var(--bg-tertiary, #374151);
  }

  .markdown-content :deep(th),
  .markdown-content :deep(td) {
    border-color: var(--border-color, #374151);
  }

  .markdown-content :deep(mark.session-search-hit) {
    background: rgba(250, 204, 21, 0.22);
  }

  .markdown-content :deep(mark.session-search-hit.active) {
    background: rgba(249, 115, 22, 0.28);
  }
}
</style>
