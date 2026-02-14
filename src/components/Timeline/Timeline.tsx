import { useState } from 'react';
import {
    Clock,
    ChevronLeft,
    ChevronRight,
    Calendar,
    Monitor,
    RefreshCw,
    Music,
} from 'lucide-react';
import { Card, CardHeader, CardContent, Button } from '../common';
import { useActivities } from '../../hooks/useActivities';
import { formatDuration, formatTime } from '../../lib/utils';

const CATEGORY_COLORS: Record<number, string> = {
    1: '#3b82f6', // Development - blue
    2: '#10b981', // Browser - green
    3: '#8b5cf6', // Communication - purple
    4: '#f59e0b', // Entertainment - amber
    5: '#ec4899', // Productivity - pink
    6: '#6b7280', // System - gray
    7: '#9ca3af', // Other - slate
};

const CATEGORY_NAMES: Record<number, string> = {
    1: 'Development',
    2: 'Browser',
    3: 'Communication',
    4: 'Entertainment',
    5: 'Productivity',
    6: 'System',
    7: 'Other',
};

export function Timeline() {
    const [daysAgo, setDaysAgo] = useState(0);
    const { activities, stats, isLoading, error, refresh } = useActivities(daysAgo);

    const goBack = () => setDaysAgo((d) => d + 1);
    const goForward = () => setDaysAgo((d) => Math.max(0, d - 1));

    const dateLabel =
        daysAgo === 0
            ? 'Today'
            : daysAgo === 1
                ? 'Yesterday'
                : `${daysAgo} days ago`;

    return (
        <div className="space-y-6">
            {/* Date Picker Row */}
            <Card variant="bordered">
                <div className="flex items-center justify-between">
                    <Button variant="ghost" size="sm" onClick={goBack}>
                        <ChevronLeft className="w-4 h-4" />
                        Previous
                    </Button>
                    <div className="flex items-center gap-2 text-white">
                        <Calendar className="w-5 h-5 text-primary-400" />
                        <span className="text-lg font-semibold">{dateLabel}</span>
                    </div>
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={goForward}
                        disabled={daysAgo === 0}
                    >
                        Next
                        <ChevronRight className="w-4 h-4" />
                    </Button>
                </div>
            </Card>

            {/* Summary Stats */}
            {stats && (
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                    <Card variant="bordered">
                        <div className="flex items-center gap-3">
                            <div className="p-3 bg-primary-600/20 rounded-lg">
                                <Clock className="w-5 h-5 text-primary-400" />
                            </div>
                            <div>
                                <p className="text-sm text-dark-400">Total Time</p>
                                <p className="text-xl font-bold text-white">
                                    {formatDuration(stats.total_duration)}
                                </p>
                            </div>
                        </div>
                    </Card>
                    <Card variant="bordered">
                        <div className="flex items-center gap-3">
                            <div className="p-3 bg-green-600/20 rounded-lg">
                                <Monitor className="w-5 h-5 text-green-400" />
                            </div>
                            <div>
                                <p className="text-sm text-dark-400">Sessions</p>
                                <p className="text-xl font-bold text-white">
                                    {stats.total_events}
                                </p>
                            </div>
                        </div>
                    </Card>
                    <Card variant="bordered">
                        <div className="flex items-center gap-3">
                            <div className="p-3 bg-purple-600/20 rounded-lg">
                                <RefreshCw className="w-5 h-5 text-purple-400" />
                            </div>
                            <div>
                                <p className="text-sm text-dark-400">Top App</p>
                                <p className="text-xl font-bold text-white truncate">
                                    {stats.top_apps[0]?.app_name || 'N/A'}
                                </p>
                            </div>
                        </div>
                    </Card>
                </div>
            )}

            {/* Loading / Error */}
            {isLoading && (
                <div className="flex items-center justify-center py-12">
                    <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-500" />
                </div>
            )}

            {error && (
                <Card variant="bordered">
                    <div className="text-center py-8 text-red-400">
                        <p>{error}</p>
                        <Button variant="secondary" size="sm" className="mt-3" onClick={refresh}>
                            Retry
                        </Button>
                    </div>
                </Card>
            )}

            {/* Activity Timeline */}
            {!isLoading && !error && (
                <Card variant="bordered">
                    <CardHeader
                        title="Activity Timeline"
                        subtitle={`${activities.length} sessions recorded`}
                        action={
                            <Button variant="ghost" size="sm" onClick={refresh}>
                                <RefreshCw className="w-4 h-4" />
                            </Button>
                        }
                    />
                    <CardContent>
                        {activities.length === 0 ? (
                            <div className="text-center py-12 text-dark-400">
                                <Clock className="w-12 h-12 mx-auto mb-4 opacity-50" />
                                <p className="text-lg">No activities recorded</p>
                                <p className="text-sm mt-1">Activities will appear here as they are tracked</p>
                            </div>
                        ) : (
                            <div className="relative">
                                {/* Timeline line */}
                                <div className="absolute left-[72px] top-0 bottom-0 w-0.5 bg-dark-700" />

                                <div className="space-y-1">
                                    {activities.map((activity, index) => {
                                        const color = CATEGORY_COLORS[activity.category_id] || CATEGORY_COLORS[8];
                                        const categoryName = CATEGORY_NAMES[activity.category_id] || 'Other';

                                        return (
                                            <div
                                                key={activity.id || index}
                                                className="flex items-start gap-4 py-2 px-2 rounded-lg hover:bg-dark-800/50 transition-colors group"
                                            >
                                                {/* Time */}
                                                <div className="flex-shrink-0 w-16 text-right">
                                                    <span className="text-sm text-primary-400 font-mono">
                                                        {formatTime(activity.start_time)}
                                                    </span>
                                                </div>

                                                {/* Dot */}
                                                <div className="flex-shrink-0 relative z-10 mt-1.5">
                                                    <div
                                                        className="w-3 h-3 rounded-full ring-2 ring-dark-900"
                                                        style={{ backgroundColor: color }}
                                                    />
                                                </div>

                                                {/* Content */}
                                                <div className="flex-1 min-w-0 pb-2">
                                                    <div className="flex items-center gap-2">
                                                        <p className="text-white font-medium truncate">
                                                            {activity.app_name}
                                                        </p>
                                                        <span
                                                            className="px-2 py-0.5 rounded-full text-xs font-medium"
                                                            style={{
                                                                backgroundColor: `${color}20`,
                                                                color: color,
                                                            }}
                                                        >
                                                            {categoryName}
                                                        </span>
                                                    </div>
                                                    {activity.window_title && (
                                                        <p className="text-sm text-dark-400 truncate mt-0.5">
                                                            {activity.window_title}
                                                        </p>
                                                    )}

                                                    {activity.metadata?.media_info && activity.metadata.media_info.status === 'Playing' && (
                                                        <div className="flex items-center gap-1.5 mt-1.5 text-xs text-emerald-400 font-medium">
                                                            <Music className="w-3 h-3 flex-shrink-0" />
                                                            <span className="truncate opacity-90">
                                                                {activity.metadata.media_info.title}
                                                                {activity.metadata.media_info.artist && (
                                                                    <>
                                                                        <span className="text-dark-500 mx-1">•</span>
                                                                        {activity.metadata.media_info.artist}
                                                                    </>
                                                                )}
                                                            </span>
                                                        </div>
                                                    )}
                                                    <p className="text-xs text-dark-500 mt-1">
                                                        {formatDuration(activity.duration_seconds)}
                                                    </p>
                                                </div>
                                            </div>
                                        );
                                    })}
                                </div>
                            </div>
                        )}
                    </CardContent>
                </Card>
            )}

            {/* Category Breakdown */}
            {stats && stats.top_categories.length > 0 && (
                <Card variant="bordered">
                    <CardHeader title="Category Breakdown" subtitle="Time spent per category" />
                    <CardContent>
                        <div className="space-y-3">
                            {stats.top_categories.map((cat) => {
                                const color = CATEGORY_COLORS[cat.category_id] || CATEGORY_COLORS[8];
                                return (
                                    <div key={cat.category_id} className="flex items-center gap-4">
                                        <div
                                            className="w-3 h-3 rounded-full flex-shrink-0"
                                            style={{ backgroundColor: color }}
                                        />
                                        <div className="flex-1">
                                            <div className="flex items-center justify-between mb-1">
                                                <span className="text-white text-sm font-medium">
                                                    {cat.category_name}
                                                </span>
                                                <span className="text-sm text-dark-400">
                                                    {formatDuration(cat.duration)}
                                                </span>
                                            </div>
                                            <div className="h-1.5 bg-dark-700 rounded-full overflow-hidden">
                                                <div
                                                    className="h-full rounded-full transition-all duration-500"
                                                    style={{
                                                        width: `${cat.percentage}%`,
                                                        backgroundColor: color,
                                                    }}
                                                />
                                            </div>
                                        </div>
                                        <span className="text-xs text-dark-500 w-10 text-right">
                                            {Math.round(cat.percentage)}%
                                        </span>
                                    </div>
                                );
                            })}
                        </div>
                    </CardContent>
                </Card>
            )}
        </div>
    );
}
