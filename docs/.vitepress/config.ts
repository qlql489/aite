import { defineConfig } from 'vitepress'

export default defineConfig({
  lang: 'zh-CN',
  title: 'Aite 文档',
  description: 'Aite 使用文档与功能说明',
  base: '/aite/',
  lastUpdated: false,
  themeConfig: {
    nav: [
      { text: '首页', link: '/' },
      { text: '快速开始', link: '/quick-start/' },
      { text: '项目与聊天', link: '/project-chat/' },
      { text: '设置', link: '/settings/' },
      { text: '下载', link: 'https://github.com/qlql489/aite/releases' },
    ],
    sidebar: [
      {
        text: '快速开始',
        items: [
          { text: '概览', link: '/quick-start/' },
          { text: '安装与环境准备', link: '/quick-start/installation' },
          { text: '自动安装', link: '/quick-start/auto-installation' },
          { text: '手动安装', link: '/quick-start/manual-installation' },
          { text: 'Claude Code 环境变量', link: '/quick-start/claude-code-env' },
        ],
      },
      {
        text: '项目与聊天功能',
        items: [
          { text: '概览', link: '/project-chat/' },
          { text: '项目导入与管理', link: '/project-chat/project-management' },
          { text: '历史会话与消息', link: '/project-chat/history-and-sessions' },
          { text: '聊天工具与运行控制', link: '/project-chat/tools-and-controls' },
          { text: '工作区与扩展能力', link: '/project-chat/workspace-and-extensions' },
        ],
      },
      {
        text: '设置',
        items: [
          { text: '概览', link: '/settings/' },
          { text: '全局命令、Skill 与 MCP', link: '/settings/global-extensions' },
          { text: '供应商与费用统计', link: '/settings/providers-and-costs' },
          { text: '外观设置', link: '/settings/appearance' },
          { text: 'CLI 参数与自动更新', link: '/settings/cli-and-updates' },
        ],
      },
    ],
    socialLinks: [{ icon: 'github', link: 'https://github.com/qlql489/aite' }],
    outline: {
      level: [2, 3],
    },
  },
})
