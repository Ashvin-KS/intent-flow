import { useMemo } from 'react';
import {
    AreaChart,
    Area,
    XAxis,
    YAxis,
    CartesianGrid,
    Tooltip,
    ResponsiveContainer,
    PieChart,
    Pie,
    Cell,
    Legend
} from 'recharts';
import { Card, CardHeader, CardContent } from '../common';
import type { Activity, ActivityStats } from '../../types';

interface ActivityChartsProps {
    stats: ActivityStats | null;
    activities: Activity[];
}

export function ActivityCharts({ stats, activities }: ActivityChartsProps) {
    // Process hourly data for the AreaChart
    const hourlyData = useMemo(() => {
        const hours = Array.from({ length: 24 }, (_, i) => ({
            hour: `${i}:00`,
            duration: 0,
            hourNum: i
        }));

        activities.forEach(activity => {
            let current = activity.start_time;
            const end = activity.end_time;

            // Handle activities that span across hour boundaries
            while (current < end) {
                const date = new Date(current * 1000);
                const hour = date.getHours();

                // Calculate start of next hour
                const nextHour = new Date(date);
                nextHour.setHours(hour + 1, 0, 0, 0);
                const nextHourStart = Math.floor(nextHour.getTime() / 1000);

                // Duration in current hour slot
                const effectEnd = Math.min(end, nextHourStart);
                const durationInSecs = effectEnd - current;

                if (hours[hour]) {
                    hours[hour].duration += durationInSecs / 60;
                }

                current = effectEnd;
            }
        });

        return hours;
    }, [activities]);

    const pieData = useMemo(() => {
        if (!stats) return [];
        return stats.top_categories.map(cat => ({
            name: cat.category_name,
            value: cat.duration / 60 // in minutes
        })).filter(cat => cat.value > 0);
    }, [stats]);

    const COLORS = ['#3b82f6', '#10b981', '#8b5cf6', '#f59e0b', '#ec4899', '#6b7280', '#9ca3af'];

    if (!stats || activities.length === 0) {
        return null;
    }

    return (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {/* Activity Timeline */}
            <Card variant="bordered">
                <CardHeader title="Activity Timeline" subtitle="Time spent per hour (minutes)" />
                <CardContent className="h-[300px]">
                    <ResponsiveContainer width="100%" height="100%">
                        <AreaChart data={hourlyData}>
                            <defs>
                                <linearGradient id="colorDuration" x1="0" y1="0" x2="0" y2="1">
                                    <stop offset="5%" stopColor="#3b82f6" stopOpacity={0.3} />
                                    <stop offset="95%" stopColor="#3b82f6" stopOpacity={0} />
                                </linearGradient>
                            </defs>
                            <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="#374151" />
                            <XAxis
                                dataKey="hour"
                                stroke="#9ca3af"
                                fontSize={12}
                                tickFormatter={(value) => value.split(':')[0]}
                                interval={2}
                            />
                            <YAxis stroke="#9ca3af" fontSize={12} />
                            <Tooltip
                                contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px', color: '#f3f4f6' }}
                                itemStyle={{ color: '#60a5fa' }}
                                formatter={(value: number | undefined) => [`${Math.round(value || 0)}m`, 'Duration']}
                            />
                            <Area
                                type="monotone"
                                dataKey="duration"
                                stroke="#3b82f6"
                                fillOpacity={1}
                                fill="url(#colorDuration)"
                            />
                        </AreaChart>
                    </ResponsiveContainer>
                </CardContent>
            </Card>

            {/* Category Distribution */}
            <Card variant="bordered">
                <CardHeader title="Time Allocation" subtitle="Distribution across categories" />
                <CardContent className="h-[300px]">
                    <ResponsiveContainer width="100%" height="100%">
                        <PieChart>
                            <Pie
                                data={pieData}
                                cx="50%"
                                cy="50%"
                                innerRadius={60}
                                outerRadius={80}
                                paddingAngle={5}
                                dataKey="value"
                            >
                                {pieData.map((_, index) => (
                                    <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
                                ))}
                            </Pie>
                            <Tooltip
                                contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px', color: '#f3f4f6' }}
                                formatter={(value: number | undefined) => [`${Math.round(value || 0)}m`, 'Time Spent']}
                            />
                            <Legend
                                verticalAlign="bottom"
                                align="center"
                                iconType="circle"
                                wrapperStyle={{ paddingTop: '20px', fontSize: '12px', color: '#9ca3af' }}
                            />
                        </PieChart>
                    </ResponsiveContainer>
                </CardContent>
            </Card>
        </div>
    );
}
