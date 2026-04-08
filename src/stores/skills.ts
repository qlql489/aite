// Skills store - 管理从文件系统读取的 skill 和 command 数据
import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export interface SkillFile {
  name: string;
  description: string;
  content: string;
  source: 'global' | 'project' | 'installed' | 'plugin';
  installedSource?: 'agents' | 'claude';
  filePath: string;
  pluginName?: string;
}

export interface SkillsResponse {
  commands: SkillFile[];
  skills: SkillFile[];
}

export interface SkillsQueryOptions {
  workspacePath?: string;
}

export const useSkillsStore = defineStore('skills', () => {
  const commands = ref<SkillFile[]>([]);
  const skills = ref<SkillFile[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  // 按 source 分组
  const commandsBySource = computed(() => {
    const grouped: Record<string, SkillFile[]> = {
      global: [],
      project: [],
      plugin: [],
    };
    for (const cmd of commands.value) {
      if (grouped[cmd.source]) {
        grouped[cmd.source].push(cmd);
      }
    }
    return grouped;
  });

  const skillsBySource = computed(() => {
    const grouped: Record<string, SkillFile[]> = {
      global: [],
      project: [],
      installed: [],
    };
    for (const skill of skills.value) {
      if (grouped[skill.source]) {
        grouped[skill.source].push(skill);
      }
    }
    return grouped;
  });

  // 获取所有 commands 和 skills
  async function loadSkills(options: SkillsQueryOptions = {}): Promise<SkillsResponse> {
    return invoke<SkillsResponse>('get_skills', {
      workspacePath: options.workspacePath,
    });
  }

  async function fetchSkills(options: SkillsQueryOptions = {}) {
    loading.value = true;
    error.value = null;
    try {
      const result = await loadSkills(options);
      commands.value = result.commands;
      skills.value = result.skills;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      console.error('[SkillsStore] Failed to fetch skills:', e);
    } finally {
      loading.value = false;
    }
  }

  // 获取单个 skill 内容
  async function fetchSkillContent(filePath: string): Promise<string> {
    try {
      return await invoke<string>('get_skill_content', { filePath });
    } catch (e) {
      console.error('[SkillsStore] Failed to fetch skill content:', e);
      throw e;
    }
  }

  // 保存 skill
  async function saveSkill(filePath: string, content: string) {
    await invoke('save_skill', { filePath, content });
    // 更新本地数据
    const entry = skills.value.find(s => s.filePath === filePath)
      ?? commands.value.find(c => c.filePath === filePath);
    if (entry) {
      entry.content = content;
      const firstLine = content.split('\n')[0]?.trim() || '';
      entry.description = firstLine.startsWith('#')
        ? firstLine.replace(/^#+\s*/, '')
        : firstLine || entry.description;
    }
  }

  // 创建 skill
  async function createSkill(
    name: string,
    content: string,
    scope: 'global' | 'project',
    options: SkillsQueryOptions = {}
  ): Promise<SkillFile> {
    const result = await invoke<SkillFile>('create_skill', {
      name,
      content,
      scope,
      workspacePath: options.workspacePath,
    });
    if (!options.workspacePath) {
      skills.value.push(result);
    }
    return result;
  }

  async function importSkillFolder(
    folderPath: string,
    scope: 'global' | 'project',
    options: SkillsQueryOptions = {}
  ): Promise<SkillFile> {
    const result = await invoke<SkillFile>('import_skill_folder', {
      folderPath,
      scope,
      workspacePath: options.workspacePath,
    });
    if (!options.workspacePath) {
      skills.value.push(result);
    }
    return result;
  }

  async function createCommand(
    name: string,
    content: string,
    scope: 'global' | 'project',
    options: SkillsQueryOptions = {}
  ): Promise<SkillFile> {
    const result = await invoke<SkillFile>('create_command', {
      name,
      content,
      scope,
      workspacePath: options.workspacePath,
    });
    if (!options.workspacePath) {
      commands.value.push(result);
    }
    return result;
  }

  async function deleteCommand(filePath: string) {
    await invoke('delete_skill', { filePath });
    commands.value = commands.value.filter(c => c.filePath !== filePath);
  }

  // 删除 skill
  async function deleteSkill(filePath: string) {
    await invoke('delete_skill', { filePath });
    skills.value = skills.value.filter(s => s.filePath !== filePath);
  }

  return {
    commands,
    skills,
    loading,
    error,
    commandsBySource,
    skillsBySource,
    loadSkills,
    fetchSkills,
    fetchSkillContent,
    saveSkill,
    createSkill,
    importSkillFolder,
    createCommand,
    deleteCommand,
    deleteSkill,
  };
});
