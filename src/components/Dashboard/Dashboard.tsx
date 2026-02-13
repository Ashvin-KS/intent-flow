import React, { useState, useEffect } from 'react';
import { 
  Activity, 
  Clock, 
  TrendingUp, 
  Zap,
  Settings,
  History,
  Target,
  BarChart3
} from 'lucide-react';
import { Card, CardHeader, CardContent, Button } from '../common';
import { QueryInput } from '../Query/QueryInput';
import { QueryResults } from '../Query/QueryResults';
import { getActivityStats, getWorkflowSuggestions } from '../../services/tauri';
import type { ActivityStats, WorkflowSuggestion, QueryResult } from '../../types';
import { formatDuration, getDayRange } from '../../lib/utils';

type TabType = 'dashboard' | 'timeline' | 'workflows' | 'settings';

export function Dashboard() {
  const [activeTab, setActiveTab] = useState<TabType>('dashboard');
  const [stats, setStats] = useState<ActivityStats | null>(null);
  const [suggestions, setSuggestions] = useState<WorkflowSuggestion[]>([]);
  const [queryResult, setQueryResult] = useState<QueryResult | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    try {
      const { start, end } = getDayRange(0);
      const [statsData, suggestionsData] = await Promise.all([
        getActivityStats(start, end),
        getWorkflowSuggestions(),
      ]);
      setStats(statsData);
      setSuggestions(suggestionsData);
    } catch (error) {
      console.error('Failed to load data:', error);
    } finally {
      setIsLoading(false);
    }
  };

  const tabs = [
    { id: 'dashboard' as const, label: 'Dashboard', icon: BarChart3 },
    { id: 'timeline' as const, label: 'Timeline', icon: History },
    { id: 'workflows' as const, label: 'Workflows', icon: Zap },
    { id: 'settings' as const, label: 'Settings', icon: Settings },
  ];

  return (
    <div className="min-h-screen bg-dark-950">
      {/* Header */}
      <header className="border-b border-dark-800 bg-dark-900/50 backdrop-blur-sm sticky top-0 z-40">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex items-center justify-between h-16">
            <div className="flex items-center gap-3">
              <div className="w-8 h-8 bg-primary-600 rounded-lg flex items-center justify-center">
                <Activity className="w-5 h-5 text-white" />
              </div>
              <h1 className="text-xl font-bold text-white">IntentFlow</h1>
            </div>
            
            <nav className="flex items-center gap-1">
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id)}
                  className={`flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                    activeTab === tab.id
                      ? 'bg-primary-600 text-white'
                      : 'text-dark-300 hover:text-white hover:bg-dark-800'
                  }`}
                >
                  <tab.icon className="w-4 h-4" />
                  {tab.label}
                </button>
              ))}
            </nav>
          </div>
        </div>
      </header>

      {/* Main content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {activeTab === 'dashboard' && (
          <DashboardContent
            stats={stats}
            suggestions={suggestions}
            isLoading={isLoading}
            onQueryResult={setQueryResult}
            queryResult={queryResult}
          />
        )}
        
        {activeTab === 'timeline' && (
          <div className="text-center py-12 text-dark-400">
            <History className="w-12 h-12 mx-auto mb-4 opacity-50" />
            <p className="text-lg">Timeline View</p>
            <p className="text-sm mt-1">Coming soon</p>
          </div>
        )}
        
        {activeTab === 'workflows' && (
          <div className="text-center py-12 text-dark-400">
            <Zap className="w-12 h-12 mx-auto mb-4 opacity-50" />
            <p className="text-lg">Workflows</p>
            <p className="text-sm mt-1">Coming soon</p>
          </div>
        )}
        
        {activeTab === 'settings' && (
          <div className="text-center py-12 text-dark-400">
            <Settings className="w-12 h-12 mx-auto mb-4 opacity-50" />
            <p className="text-lg">Settings</p>
            <p className="text-sm mt-1">Coming soon</p>
          </div>
        )}
      </main>
    </div>
  );
}

interface DashboardContentProps {
  stats: ActivityStats | null;
  suggestions: WorkflowSuggestion[];
  isLoading: boolean;
  onQueryResult: (result: QueryResult) => void;
  queryResult: QueryResult | null;
}

function DashboardContent({
  stats,
  suggestions,
  isLoading,
  onQueryResult,
  queryResult,
}: DashboardContentProps) {
  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-500" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Query Section */}
      <Card variant="bordered">
        <CardHeader title="Ask Anything" subtitle="Query your activity history" />
        <CardContent>
          <QueryInput onResult={onQueryResult} />
          <div className="mt-6">
            <QueryResults result={queryResult} />
          </div>
        </CardContent>
      </Card>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card variant="bordered">
          <div className="flex items-center gap-4">
            <div className="p-3 bg-primary-600/20 rounded-lg">
              <Clock className="w-6 h-6 text-primary-400" />
            </div>
            <div>
              <p className="text-sm text-dark-400">Total Time Today</p>
              <p className="text-2xl font-bold text-white">
                {stats ? formatDuration(stats.total_duration) : '0h 0m'}
              </p>
            </div>
          </div>
        </Card>

        <Card variant="bordered">
          <div className="flex items-center gap-4">
            <div className="p-3 bg-green-600/20 rounded-lg">
              <Activity className="w-6 h-6 text-green-400" />
            </div>
            <div>
              <p className="text-sm text-dark-400">Activities</p>
              <p className="text-2xl font-bold text-white">
                {stats?.total_events || 0}
              </p>
            </div>
          </div>
        </Card>

        <Card variant="bordered">
          <div className="flex items-center gap-4">
            <div className="p-3 bg-purple-600/20 rounded-lg">
              <TrendingUp className="w-6 h-6 text-purple-400" />
            </div>
            <div>
              <p className="text-sm text-dark-400">Top Category</p>
              <p className="text-2xl font-bold text-white">
                {stats?.top_categories[0]?.category_name || 'N/A'}
              </p>
            </div>
          </div>
        </Card>
      </div>

      {/* Top Apps */}
      {stats && stats.top_apps.length > 0 && (
        <Card variant="bordered">
          <CardHeader title="Top Apps Today" subtitle="Most used applications" />
          <CardContent>
            <div className="space-y-3">
              {stats.top_apps.slice(0, 5).map((app, index) => (
                <div key={app.app_name} className="flex items-center gap-4">
                  <span className="text-sm text-dark-400 w-6">{index + 1}.</span>
                  <div className="flex-1">
                    <div className="flex items-center justify-between mb-1">
                      <span className="text-white font-medium">{app.app_name}</span>
                      <span className="text-sm text-dark-400">
                        {formatDuration(app.duration)}
                      </span>
                    </div>
                    <div className="h-2 bg-dark-700 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-primary-600 rounded-full"
                        style={{ width: `${app.percentage}%` }}
                      />
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Workflow Suggestions */}
      {suggestions.length > 0 && (
        <Card variant="bordered">
          <CardHeader 
            title="Suggested Workflows" 
            subtitle="Based on your patterns"
            action={
              <Button variant="ghost" size="sm">
                View All
              </Button>
            }
          />
          <CardContent>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              {suggestions.slice(0, 4).map((suggestion) => (
                <button
                  key={suggestion.workflow.id}
                  className="flex items-center gap-3 p-3 bg-dark-800 rounded-lg hover:bg-dark-700 transition-colors text-left"
                >
                  <div className="p-2 bg-primary-600/20 rounded-lg">
                    <Target className="w-5 h-5 text-primary-400" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-white font-medium truncate">
                      {suggestion.workflow.name}
                    </p>
                    <p className="text-xs text-dark-400 truncate">
                      {suggestion.reason}
                    </p>
                  </div>
                  <span className="text-xs text-dark-500">
                    {Math.round(suggestion.relevance_score * 100)}% match
                  </span>
                </button>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
