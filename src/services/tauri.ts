import { invoke } from '@tauri-apps/api/core';
import type {
  Activity,
  ActivityStats,
  Category,
  ManualEntry,
  Pattern,
  Intent,
  Workflow,
  WorkflowSuggestion,
  QueryResult,
  Settings,
  StorageStats,
  ChatSession,
  ChatMessage,
  DashboardOverview,
} from '../types';

// Activity commands
export async function getActivities(
  startTime: number,
  endTime: number,
  limit?: number
): Promise<Activity[]> {
  return invoke('get_activities', { startTime, endTime, limit });
}

export async function getActivityStats(
  startTime: number,
  endTime: number
): Promise<ActivityStats> {
  return invoke('get_activity_stats', { startTime, endTime });
}

export async function getCurrentActivity(): Promise<Activity | null> {
  return invoke('get_current_activity');
}

// Query commands
export async function executeQuery(query: string): Promise<QueryResult> {
  return invoke('execute_query', { query });
}

export async function getQueryHistory(limit?: number): Promise<QueryResult[]> {
  return invoke('get_query_history', { limit });
}

// Intent commands
export async function parseIntent(input: string): Promise<Intent> {
  return invoke('parse_intent', { input });
}

export async function executeIntent(intent: Intent): Promise<void> {
  return invoke('execute_intent', { intent });
}

// Workflow commands
export async function getWorkflows(): Promise<Workflow[]> {
  return invoke('get_workflows');
}

export async function createWorkflow(workflow: Omit<Workflow, 'id' | 'use_count' | 'last_used' | 'created_at'>): Promise<string> {
  return invoke('create_workflow', { workflow });
}

export async function updateWorkflow(workflow: Workflow): Promise<void> {
  return invoke('update_workflow', { workflow });
}

export async function deleteWorkflow(workflowId: string): Promise<void> {
  return invoke('delete_workflow', { workflowId });
}

export async function executeWorkflow(workflowId: string): Promise<void> {
  return invoke('execute_workflow', { workflowId });
}

export async function getWorkflowSuggestions(): Promise<WorkflowSuggestion[]> {
  return invoke('get_workflow_suggestions');
}

// Manual entry commands
export async function createEntry(
  entryType: 'task' | 'note' | 'goal',
  title: string,
  content?: string,
  tags?: string[]
): Promise<string> {
  return invoke('create_entry', { entryType, title, content, tags });
}

export async function getEntries(
  entryType?: 'task' | 'note' | 'goal',
  status?: 'active' | 'completed' | 'archived',
  limit?: number
): Promise<ManualEntry[]> {
  return invoke('get_entries', { entryType, status, limit });
}

export async function updateEntryStatus(
  id: number,
  status: 'active' | 'completed' | 'archived'
): Promise<void> {
  return invoke('update_entry_status', { id, status });
}

export async function deleteEntry(id: number): Promise<void> {
  return invoke('delete_entry', { id });
}

// Pattern commands
export async function getPatterns(): Promise<Pattern[]> {
  return invoke('get_patterns');
}

// Settings commands
export async function getSettings(): Promise<Settings> {
  return invoke('get_settings');
}

export async function updateSettings(settings: Partial<Settings>): Promise<void> {
  return invoke('update_settings', { settings });
}

export async function getCategories(): Promise<Category[]> {
  return invoke('get_categories');
}

export async function updateCategories(categories: Category[]): Promise<void> {
  return invoke('update_categories', { categories });
}

export interface ModelInfo {
  id: string;
  name: string;
}

export interface RecentModel {
  id: string;
  name: string;
  use_count: number;
  last_used: number;
}

export async function getNvidiaModels(apiKey: string): Promise<ModelInfo[]> {
  return invoke('get_nvidia_models', { apiKey });
}

export async function getRecentModels(limit = 5): Promise<RecentModel[]> {
  return invoke('get_recent_models', { limit });
}

export async function removeRecentModel(modelId: string): Promise<void> {
  return invoke('remove_recent_model', { modelId });
}

// Storage commands
export async function getStorageStats(): Promise<StorageStats> {
  return invoke('get_storage_stats');
}

export async function cleanupOldData(retentionDays: number): Promise<number> {
  return invoke('cleanup_old_data', { retentionDays });
}

export async function exportData(): Promise<string> {
  return invoke('export_data');
}

// App control commands
export async function minimizeToTray(): Promise<void> {
  return invoke('minimize_to_tray');
}

export async function showWindow(): Promise<void> {
  return invoke('show_window');
}

export async function quitApp(): Promise<void> {
  return invoke('quit_app');
}

// Chat commands
export async function createChatSession(): Promise<ChatSession> {
  return invoke('create_chat_session');
}

export async function getChatSessions(): Promise<ChatSession[]> {
  return invoke('get_chat_sessions');
}

export async function deleteChatSession(sessionId: string): Promise<void> {
  return invoke('delete_chat_session', { sessionId });
}

export async function getChatMessages(sessionId: string): Promise<ChatMessage[]> {
  return invoke('get_chat_messages', { sessionId });
}

export async function sendChatMessage(
  sessionId: string,
  message: string,
  model?: string,
  timeRange?: string,
  selectedSources?: string[]
): Promise<ChatMessage> {
  return invoke('send_chat_message', { sessionId, message, model, timeRange, selectedSources });
}

// Dashboard commands
export async function getDashboardOverview(refresh = false): Promise<DashboardOverview> {
  return invoke('get_dashboard_overview', { refresh });
}

export async function refreshDashboardOverview(): Promise<DashboardOverview> {
  return invoke('refresh_dashboard_overview');
}

export async function summarizeContact(name: string): Promise<string> {
  return invoke('summarize_contact', { name });
}

export async function summarizeProject(name: string): Promise<string> {
  return invoke('summarize_project', { name });
}

