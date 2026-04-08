use super::{
    get_all_projects_statistics_impl, get_project_statistics_impl, get_statistics_projects_impl,
    ProjectInfo, ProjectStatistics, TrendData, UsageData, WeekData, WeeklyComparison,
};
/**
 * 统计数据缓存管理
 * 支持异步加载和定时刷新
 */
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::time::interval;

/// 缓存条目
#[derive(Debug, Clone)]
struct CacheEntry {
    data: ProjectStatistics,
    timestamp: i64,
    loading: bool,
}

/// 统计数据缓存状态
pub struct StatsCache {
    /// 项目统计缓存 (key: project_path)
    project_cache: RwLock<HashMap<String, CacheEntry>>,
    /// 所有项目统计缓存
    all_projects_cache: RwLock<Option<CacheEntry>>,
    /// 可用项目列表缓存
    projects_list_cache: RwLock<Option<(Vec<ProjectInfo>, i64)>>,
    /// 缓存有效期（毫秒）
    cache_duration: i64,
}

impl StatsCache {
    pub fn new() -> Self {
        Self {
            project_cache: RwLock::new(HashMap::new()),
            all_projects_cache: RwLock::new(None),
            projects_list_cache: RwLock::new(None),
            cache_duration: 10 * 60 * 1000, // 10分钟
        }
    }

    /// 检查缓存是否有效
    fn is_cache_valid(&self, timestamp: i64) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        (now - timestamp) < self.cache_duration
    }

    /// 获取项目统计（带缓存）
    pub fn get_project_stats(&self, project_path: &str) -> Result<ProjectStatistics, String> {
        let cache = self.project_cache.read().unwrap();
        if let Some(entry) = cache.get(project_path) {
            if self.is_cache_valid(entry.timestamp) && !entry.loading {
                return Ok(entry.data.clone());
            }
        }
        drop(cache);

        // 缓存无效，触发异步刷新
        self.refresh_project(project_path);

        // 返回缓存数据（如果存在）或错误
        let cache = self.project_cache.read().unwrap();
        if let Some(entry) = cache.get(project_path) {
            if entry.loading {
                return Err("loading".to_string());
            }
            return Ok(entry.data.clone());
        }
        Err("loading".to_string())
    }

    /// 获取所有项目统计（带缓存）
    pub fn get_all_projects_stats(&self) -> Result<ProjectStatistics, String> {
        let cache = self.all_projects_cache.read().unwrap();
        if let Some(entry) = cache.as_ref() {
            if self.is_cache_valid(entry.timestamp) && !entry.loading {
                return Ok(entry.data.clone());
            }
        }
        drop(cache);

        // 缓存无效，触发异步刷新
        self.refresh_all_projects();

        // 返回缓存数据（如果存在）或错误
        let cache = self.all_projects_cache.read().unwrap();
        if let Some(entry) = cache.as_ref() {
            if entry.loading {
                return Err("loading".to_string());
            }
            return Ok(entry.data.clone());
        }
        Err("loading".to_string())
    }

    /// 获取项目列表（带缓存）
    pub fn get_projects_list(&self) -> Result<Vec<ProjectInfo>, String> {
        let cache = self.projects_list_cache.read().unwrap();
        if let Some((projects, timestamp)) = cache.as_ref() {
            if self.is_cache_valid(*timestamp) {
                return Ok(projects.clone());
            }
        }
        drop(cache);

        // 缓存无效，直接加载
        let projects = get_statistics_projects_impl()?;
        let mut cache = self.projects_list_cache.write().unwrap();
        *cache = Some((projects.clone(), chrono::Utc::now().timestamp_millis()));
        Ok(projects)
    }

    /// 获取加载状态
    pub fn is_loading(&self, project_path: Option<&str>) -> bool {
        if let Some(path) = project_path {
            let cache = self.project_cache.read().unwrap();
            cache.get(path).map(|e| e.loading).unwrap_or(false)
        } else {
            let cache = self.all_projects_cache.read().unwrap();
            cache.as_ref().map(|e| e.loading).unwrap_or(false)
        }
    }

    /// 刷新单个项目缓存
    fn refresh_project(&self, project_path: &str) {
        let mut cache = self.project_cache.write().unwrap();
        let entry = cache
            .entry(project_path.to_string())
            .or_insert_with(|| CacheEntry {
                data: ProjectStatistics::empty(project_path),
                timestamp: 0,
                loading: false,
            });

        if entry.loading {
            return; // 已在加载中
        }

        entry.loading = true;
        drop(cache);

        // 异步加载数据
        let project_path = project_path.to_string();
        std::thread::spawn(move || {
            if let Ok(data) = get_project_statistics_impl(&project_path) {
                let mut cache = Self::global().project_cache.write().unwrap();
                if let Some(entry) = cache.get_mut(&project_path) {
                    entry.data = data;
                    entry.timestamp = chrono::Utc::now().timestamp_millis();
                    entry.loading = false;
                }
            } else {
                let mut cache = Self::global().project_cache.write().unwrap();
                if let Some(entry) = cache.get_mut(&project_path) {
                    entry.loading = false;
                }
            }
        });
    }

    /// 刷新所有项目缓存
    fn refresh_all_projects(&self) {
        let mut cache = self.all_projects_cache.write().unwrap();
        let entry = cache.get_or_insert_with(|| CacheEntry {
            data: ProjectStatistics::empty_all(),
            timestamp: 0,
            loading: false,
        });

        if entry.loading {
            return; // 已在加载中
        }

        entry.loading = true;
        drop(cache);

        // 异步加载数据
        std::thread::spawn(move || {
            if let Ok(data) = get_all_projects_statistics_impl() {
                let mut cache = Self::global().all_projects_cache.write().unwrap();
                if let Some(entry) = cache.as_mut() {
                    entry.data = data;
                    entry.timestamp = chrono::Utc::now().timestamp_millis();
                    entry.loading = false;
                }
            } else {
                let mut cache = Self::global().all_projects_cache.write().unwrap();
                if let Some(entry) = cache.as_mut() {
                    entry.loading = false;
                }
            }
        });
    }

    /// 预热所有缓存
    pub fn warmup(&self) {
        // 异步预热所有项目列表
        std::thread::spawn(move || {
            let _ = Self::global().get_projects_list();
        });

        // 异步预热所有项目统计
        self.refresh_all_projects();
    }

    /// 启动定时刷新任务
    pub fn start_auto_refresh(&self) {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(10 * 60)); // 10分钟
                Self::global().warmup();
            }
        });
    }

    /// 获取全局实例
    pub fn global() -> &'static Self {
        use std::sync::OnceLock;
        static GLOBAL: OnceLock<StatsCache> = OnceLock::new();
        GLOBAL.get_or_init(|| StatsCache::new())
    }
}

// 为 ProjectStatistics 添加 empty 方法
impl ProjectStatistics {
    pub fn empty(project_path: &str) -> Self {
        Self {
            project_path: project_path.to_string(),
            project_name: "Loading...".to_string(),
            total_sessions: 0,
            total_usage: UsageData {
                input_tokens: 0,
                output_tokens: 0,
                cache_write_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 0,
            },
            estimated_cost: 0.0,
            sessions: Vec::new(),
            daily_usage: Vec::new(),
            weekly_comparison: WeeklyComparison {
                current_week: WeekData {
                    sessions: 0,
                    cost: 0.0,
                    tokens: 0,
                },
                last_week: WeekData {
                    sessions: 0,
                    cost: 0.0,
                    tokens: 0,
                },
                trends: TrendData {
                    sessions: 0.0,
                    cost: 0.0,
                    tokens: 0.0,
                },
            },
            by_model: Vec::new(),
            last_updated: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn empty_all() -> Self {
        Self::empty("all")
    }
}
