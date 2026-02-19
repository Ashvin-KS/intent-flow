import { useState, useEffect } from 'react';
import {
    Activity,
    Clock,
    TrendingUp,
    Zap,
    Settings,
    History,
    Target,
    BarChart3,
    MessageCircle,
} from 'lucide-react';
import { Card, CardHeader, CardContent, Button } from '../common';
import { QueryInput } from '../Query/QueryInput';
import { QueryResults } from '../Query/QueryResults';
import { IntentInput } from '../QuickActions/IntentInput';
import { Timeline } from '../Timeline/Timeline';
import { WorkflowList } from '../Workflows/WorkflowList';
import { SettingsPanel } from '../Settings/Settings';
import { ChatPage } from '../Chat/ChatPage';
import { getActivityStats, getWorkflowSuggestions, getActivities, getRecentFiles, getClipboardHistory, getCurrentActivity } from '../../services/tauri';
import type { ActivityStats, WorkflowSuggestion, QueryResult, Activity as ActivityType } from '../../types';
import { formatDuration, getDayRange, getRelativeTime } from '../../lib/utils';
import { ActivityCharts } from './ActivityCharts';

type TabType = 'dashboard' | 'timeline' | 'chat' | 'workflows' | 'settings';

export function Dashboard() {
    const [activeTab, setActiveTab] = useState<TabType>('dashboard');
    const [stats, setStats] = useState<ActivityStats | null>(null);
    const [activities, setActivities] = useState<ActivityType[]>([]);
    const [recentFiles, setRecentFiles] = useState<string[]>([]);
    const [clipboardHistory, setClipboardHistory] = useState<[string, number][]>([]);
    const [suggestions, setSuggestions] = useState<WorkflowSuggestion[]>([]);
    const [queryResult, setQueryResult] = useState<QueryResult | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [currentActivity, setCurrentActivity] = useState<ActivityType | null>(null);
    const [lastRefreshed, setLastRefreshed] = useState<Date>(new Date());

    useEffect(() => {
        loadData();

        // Auto-refresh data every 30 seconds
        const intervalId = setInterval(() => {
            loadData(false); // silent refresh (no loading spinner)
        }, 30_000);

        // Poll current live activity every 5 seconds
        const liveInterval = setInterval(async () => {
            try {
                const current = await getCurrentActivity();
                setCurrentActivity(current);
            } catch (_) { /* ignore */ }
        }, 5_000);

        return () => {
            clearInterval(intervalId);
            clearInterval(liveInterval);
        };
    }, []);

    const loadData = async (showLoading = true) => {
        if (showLoading) setIsLoading(true);
        try {
            const { start, end } = getDayRange(0);
            const [statsData, suggestionsData, activitiesData, recentFilesData, clipboardData, currentAct] = await Promise.all([
                getActivityStats(start, end),
                getWorkflowSuggestions(),
                getActivities(start, end),
                getRecentFiles(10),
                getClipboardHistory(5),
                getCurrentActivity(),
            ]);
            setStats(statsData);
            setSuggestions(suggestionsData);
            setActivities(activitiesData);
            setRecentFiles(recentFilesData);
            setClipboardHistory(clipboardData);
            setCurrentActivity(currentAct);
            setLastRefreshed(new Date());
        } catch (error) {
            console.error('Failed to load data:', error);
        } finally {
            if (showLoading) setIsLoading(false);
        }
    };

    const tabs = [
        { id: 'dashboard' as const, label: 'Dashboard', icon: BarChart3 },
        { id: 'timeline' as const, label: 'Timeline', icon: History },
        { id: 'chat' as const, label: 'Chat', icon: MessageCircle },
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
                                    className={`flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-colors ${activeTab === tab.id
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

            {/* Main content (hidden on chat tab) */}
            {activeTab !== 'chat' && (
                <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
                    {activeTab === 'dashboard' && (
                        <DashboardContent
                            stats={stats}
                            activities={activities}
                            recentFiles={recentFiles}
                            clipboardHistory={clipboardHistory}
                            suggestions={suggestions}
                            isLoading={isLoading}
                            onQueryResult={setQueryResult}
                            queryResult={queryResult}
                            currentActivity={currentActivity}
                            lastRefreshed={lastRefreshed}
                            onRefresh={() => loadData(false)}
                        />
                    )}

                    {activeTab === 'timeline' && <Timeline />}
                    {activeTab === 'workflows' && <WorkflowList />}
                    {activeTab === 'settings' && <SettingsPanel />}
                </main>
            )}

            {/* Chat renders full-width outside the max-w container */}
            {activeTab === 'chat' && <ChatPage />}
        </div>
    );
}

interface DashboardContentProps {
    stats: ActivityStats | null;
    activities: ActivityType[];
    recentFiles: string[];
    clipboardHistory: [string, number][];
    suggestions: WorkflowSuggestion[];
    isLoading: boolean;
    onQueryResult: (result: QueryResult) => void;
    queryResult: QueryResult | null;
    currentActivity: ActivityType | null;
    lastRefreshed: Date;
    onRefresh: () => void;
}

function DashboardContent({
    stats,
    activities,
    recentFiles,
    clipboardHistory,
    suggestions,
    isLoading,
    onQueryResult,
    queryResult,
    currentActivity,
    lastRefreshed,
    onRefresh,
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
            {/* Live Activity Banner */}
            <div className="flex items-center justify-between">
                {currentActivity ? (
                    <div className="flex items-center gap-3 bg-primary-600/10 border border-primary-600/30 rounded-xl px-4 py-3 flex-1 mr-4">
                        <span className="relative flex h-3 w-3">
                            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                            <span className="relative inline-flex rounded-full h-3 w-3 bg-green-500"></span>
                        </span>
                        <div>
                            <p className="text-sm text-white font-semibold">{currentActivity.app_name}</p>
                            <p className="text-xs text-dark-400 truncate max-w-sm">{currentActivity.window_title || 'Active'}</p>
                        </div>
                        <span className="ml-auto text-xs text-dark-500">{getRelativeTime(currentActivity.start_time)}</span>
                    </div>
                ) : (
                    <div className="flex items-center gap-2 text-dark-500 text-sm">
                        <span className="inline-flex h-2 w-2 rounded-full bg-dark-600"></span>
                        No active window detected
                    </div>
                )}
                <button
                    onClick={onRefresh}
                    className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-dark-800 hover:bg-dark-700 text-dark-400 hover:text-white text-xs transition-colors shrink-0"
                    title="Refresh data"
                >
                    <TrendingUp className="w-3.5 h-3.5" />
                    Refreshed {lastRefreshed.toLocaleTimeString()}
                </button>
            </div>

            {/* Intent Input */}
            <Card variant="bordered">
                <CardHeader title="Quick Actions" subtitle="Tell IntentFlow what you want to do" />
                <CardContent>
                    <IntentInput />
                </CardContent>
            </Card>

            {/* Charts Section */}
            <ActivityCharts stats={stats} activities={activities} />

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

            {/* Recent Documents */}
            {recentFiles.length > 0 && (
                <Card variant="bordered">
                    <CardHeader title="Recent Documents" subtitle="Quick access to your work" />
                    <CardContent>
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                            {recentFiles.map((file, index) => (
                                <button
                                    key={`${file}-${index}`}
                                    className="flex items-center gap-3 p-3 bg-dark-800 rounded-lg hover:bg-dark-700 transition-colors text-left"
                                >
                                    <div className="p-2 bg-blue-600/20 rounded-lg">
                                        <Clock className="w-5 h-5 text-blue-400" />
                                    </div>
                                    <div className="flex-1 min-w-0">
                                        <p className="text-white font-medium truncate">
                                            {file}
                                        </p>
                                    </div>
                                </button>
                            ))}
                        </div>
                    </CardContent>
                </Card>
            )}

            {/* Recent Clipboard */}
            {clipboardHistory.length > 0 && (
                <Card variant="bordered">
                    <CardHeader title="Recent Clipboard" subtitle="Your recently copied snippets" />
                    <CardContent>
                        <div className="space-y-3">
                            {clipboardHistory.map(([content, timestamp], index) => (
                                <div
                                    key={`${timestamp}-${index}`}
                                    className="flex items-start gap-3 p-3 bg-dark-800 rounded-lg hover:bg-dark-700 transition-colors group cursor-default"
                                >
                                    <div className="p-2 bg-green-600/20 rounded-lg shrink-0">
                                        <History className="w-5 h-5 text-green-400" />
                                    </div>
                                    <div className="flex-1 min-w-0">
                                        <p className="text-white text-sm break-words whitespace-pre-wrap">
                                            {content.length > 200 ? content.slice(0, 200) + '...' : content}
                                        </p>
                                        <p className="text-xs text-dark-500 mt-1">
                                            {new Date(timestamp * 1000).toLocaleTimeString()}
                                        </p>
                                    </div>
                                    <Button
                                        variant="ghost"
                                        size="sm"
                                        className="opacity-0 group-hover:opacity-100 transition-opacity"
                                        onClick={() => navigator.clipboard.writeText(content)}
                                    >
                                        Copy
                                    </Button>
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