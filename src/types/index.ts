// Activity types
export interface Activity {
  id: number;
  app_name: string;
  app_hash: number;
  window_title: string;
  window_title_hash: number;
  category_id: number;
  start_time: number;
  end_time: number;
  duration_seconds: number;
  metadata?: ActivityMetadata;
}

export interface ActivityMetadata {
  is_idle: boolean;
  is_fullscreen: boolean;
  process_id?: number;
  url?: string;
  screen_text?: string;
  background_windows?: string[];
  media_info?: MediaInfo;
}

export interface MediaInfo {
  title: string;
  artist: string;
  status: string;
}

export interface ActivityStats {
  total_duration: number;
  total_events: number;
  top_apps: AppStat[];
  top_categories: CategoryStat[];
}

export interface AppStat {
  app_name: string;
  duration: number;
  count: number;
  percentage: number;
}

export interface CategoryStat {
  category_id: number;
  category_name: string;
  duration: number;
  count: number;
  percentage: number;
}

// Category types
export interface Category {
  id: number;
  name: string;
  icon: string;
  color: string;
  keywords: string[];
  apps: string[];
}

// Manual entry types
export interface ManualEntry {
  id: number;
  entry_type: 'task' | 'note' | 'goal';
  title: string;
  content?: string;
  tags: string[];
  status: 'active' | 'completed' | 'archived';
  created_at: number;
  updated_at: number;
  completed_at?: number;
}

// Pattern types
export interface Pattern {
  id: number;
  pattern_type: 'time' | 'sequence' | 'context' | 'mood';
  pattern_data: PatternData;
  confidence: number;
  last_observed: number;
  occurrence_count: number;
}

export interface PatternData {
  // Time-based pattern
  hour?: number;
  day_of_week?: number;
  likely_activities?: string[];

  // Sequence pattern
  sequence?: string[];
  avg_gap_seconds?: number;

  // Context pattern
  trigger_activity?: string;
  following_activities?: string[];

  // Mood pattern
  idle_duration?: number;
  time_of_day?: number;
  likely_intent?: string;
}

// Intent types
export interface Intent {
  intent_type: IntentType;
  confidence: number;
  parameters: Record<string, string>;
  suggested_actions: Action[];
}

export type IntentType =
  | 'work_start'
  | 'entertainment'
  | 'focus'
  | 'learning'
  | 'wind_down'
  | 'query'
  | 'unknown';

export interface Action {
  action_type: ActionType;
  target: string;
  args: string[];
}

export type ActionType =
  | 'launch_app'
  | 'open_url'
  | 'open_file'
  | 'close_app'
  | 'show_notification'
  | 'execute_workflow';

// Workflow types
export interface Workflow {
  id: string;
  name: string;
  description: string;
  icon: string;
  apps: AppLaunch[];
  urls: string[];
  files: string[];
  use_count: number;
  last_used: number;
  created_at: number;
}

export interface AppLaunch {
  path: string;
  args: string[];
}

export interface WorkflowSuggestion {
  workflow: Workflow;
  trigger_type: 'time' | 'intent' | 'pattern' | 'context';
  relevance_score: number;
  reason: string;
}

// Query types
export interface QueryResult {
  query: string;
  results: QueryItem[];
  summary: string;
  timestamp: number;
}

export interface QueryItem {
  timestamp: number;
  time_str: string;
  activity: string;
  duration: string;
  details?: string;
}

// Settings types
export interface Settings {
  version: string;
  general: GeneralSettings;
  tracking: TrackingSettings;
  storage: StorageSettings;
  ai: AISettings;
  privacy: PrivacySettings;
  notifications: NotificationSettings;
}

export interface GeneralSettings {
  language: string;
  theme: 'light' | 'dark' | 'system';
  startup_behavior: 'show_window' | 'minimized_to_tray' | 'hidden';
  minimize_to_tray: boolean;
  close_to_tray: boolean;
}

export interface TrackingSettings {
  enabled: boolean;
  tracking_interval: number;
  idle_timeout: number;
  exclude_apps: string[];
  exclude_urls: string[];
  track_browser: boolean;
}

export interface StorageSettings {
  retention_days: number;
  auto_cleanup: boolean;
  compression_enabled: boolean;
  max_cache_size_mb: number;
}

export interface AISettings {
  enabled: boolean;
  provider: 'openai' | 'anthropic' | 'local';
  api_key: string;
  model: string;
  local_only: boolean;
  fallback_to_local: boolean;
}

export interface PrivacySettings {
  encrypt_database: boolean;
  exclude_incognito: boolean;
  anonymize_data: boolean;
}

export interface NotificationSettings {
  workflow_suggestions: boolean;
  pattern_insights: boolean;
  daily_summary: boolean;
  summary_time: string;
}

// Storage stats
export interface StorageStats {
  total_size_bytes: number;
  activities_count: number;
  summaries_count: number;
  patterns_count: number;
  entries_count: number;
  oldest_activity: number;
  newest_activity: number;
}

// Chat types
export interface ChatSession {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
}

export interface AgentStep {
  turn: number;
  tool_name: string;
  tool_args: Record<string, any>;
  tool_result: string;
  reasoning: string;
}

export interface ActivityRef {
  app: string;
  title: string;
  time: number;
  duration_seconds: number;
  category: string;
  media?: { title: string; artist: string; status: string } | null;
  ocr_snippet?: string;
}

export interface ChatMessage {
  id: number;
  session_id: string;
  role: 'user' | 'assistant' | 'tool';
  content: string;
  tool_calls?: AgentStep[];
  activities?: ActivityRef[];
  created_at: number;
}

// Dashboard types
export interface DashboardTask {
  title: string;
  due_date?: string;
  status: string;
  source: string;
}

export interface ProjectOverview {
  name: string;
  update: string;
  files_changed: number;
}

export interface ContactOverview {
  name: string;
  context: string;
  last_seen?: number;
}

export interface DashboardOverview {
  date_key: string;
  summary: string;
  focus_points: string[];
  deadlines: DashboardTask[];
  projects: ProjectOverview[];
  contacts: ContactOverview[];
  updated_at: number;
}
