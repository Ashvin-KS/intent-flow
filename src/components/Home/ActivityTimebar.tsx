import { useMemo } from 'react';
import type { ActivityStats } from '../../types';

interface ActivityTimebarProps {
    stats: ActivityStats | null;
}

export function ActivityTimebar({ stats }: ActivityTimebarProps) {
    // Generate activity "dots" along a 24-hour timeline
    const timeLabels = ['12am', '6am', '12pm', '6pm', '12am'];

    // Generate a simple SVG path representing activity intensity over time
    const activityPath = useMemo(() => {
        if (!stats || stats.total_events === 0) {
            return null;
        }

        // Create a smooth'ish line from the current hour
        const now = new Date();
        const currentHour = now.getHours() + now.getMinutes() / 60;
        const width = 100; // percentage based

        // Generate points - represent activity as a gentle curve up to current time
        const points: [number, number][] = [];
        const segments = 48; // half-hour segments

        for (let i = 0; i <= segments; i++) {
            const hour = (i / segments) * 24;
            const x = (hour / 24) * width;

            if (hour > currentHour) {
                // Future: flat line at bottom
                points.push([x, 95]);
            } else {
                // Past: generate some "activity" based on typical patterns
                // Morning ramp up, afternoon steady, evening wind down
                let intensity = 0;
                if (hour >= 6 && hour < 9) intensity = ((hour - 6) / 3) * 0.6;
                else if (hour >= 9 && hour < 12) intensity = 0.6 + Math.sin((hour - 9) * 0.5) * 0.3;
                else if (hour >= 12 && hour < 14) intensity = 0.3;
                else if (hour >= 14 && hour < 18) intensity = 0.5 + Math.sin((hour - 14) * 0.4) * 0.3;
                else if (hour >= 18 && hour < 22) intensity = Math.max(0, 0.5 - (hour - 18) * 0.125);
                else intensity = 0.05;

                // Add some randomness
                intensity += Math.sin(hour * 2.7) * 0.1;
                intensity = Math.max(0.02, Math.min(1, intensity));

                const y = 95 - intensity * 80;
                points.push([x, y]);
            }
        }

        // Build SVG path
        if (points.length < 2) return null;

        let d = `M ${points[0][0]} ${points[0][1]}`;
        for (let i = 1; i < points.length; i++) {
            // Smooth curve
            const prev = points[i - 1];
            const curr = points[i];
            const cpx = (prev[0] + curr[0]) / 2;
            d += ` C ${cpx} ${prev[1]}, ${cpx} ${curr[1]}, ${curr[0]} ${curr[1]}`;
        }

        return d;
    }, [stats]);

    // Current time position
    const now = new Date();
    const currentHour = now.getHours() + now.getMinutes() / 60;
    const currentPosition = (currentHour / 24) * 100;

    return (
        <div className="flex-shrink-0 px-6 lg:px-12 pb-4">
            <div className="max-w-3xl mx-auto">
                <div className="relative h-24">
                    {/* SVG Timeline */}
                    <svg
                        className="w-full h-16"
                        viewBox="0 0 100 100"
                        preserveAspectRatio="none"
                    >
                        {/* Activity line */}
                        {activityPath && (
                            <path
                                d={activityPath}
                                fill="none"
                                stroke="#4b5563"
                                strokeWidth="1"
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                className="opacity-60"
                            />
                        )}

                        {/* Current time indicator dot */}
                        <circle
                            cx={currentPosition}
                            cy="50"
                            r="2"
                            fill="#10b981"
                            className="animate-pulse"
                        />
                    </svg>

                    {/* Current time vertical line */}
                    <div
                        className="absolute top-0 h-16 w-px bg-dark-600"
                        style={{ left: `${currentPosition}%` }}
                    >
                        <div className="absolute -top-0.5 left-1/2 -translate-x-1/2 w-2.5 h-2.5 rounded-full bg-emerald-500 border-2 border-dark-950" />
                    </div>

                    {/* Time labels */}
                    <div className="absolute bottom-0 left-0 right-0 flex justify-between">
                        {timeLabels.map((label, i) => (
                            <span
                                key={`${label}-${i}`}
                                className="text-[10px] text-dark-500"
                            >
                                {label}
                            </span>
                        ))}
                    </div>
                </div>
            </div>
        </div>
    );
}
