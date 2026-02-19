import { useState, useEffect } from 'react';
import type { PageType } from '../../App';
import {
    Sun,
    CalendarCheck,
    SlidersHorizontal,
    AlertCircle,
    Brain,
    Compass,
    MessageSquare,
    ArrowRight,
    RefreshCw,
    Lock,
} from 'lucide-react';
import { ActivityTimebar } from './ActivityTimebar';
import { getActivityStats } from '../../services/tauri';
import type { ActivityStats } from '../../types';
import { getDayRange } from '../../lib/utils';

interface HomePageProps {
    onNavigate: (page: PageType) => void;
    onChatWithPrompt: (prompt: string) => void;
}

function getGreeting(): string {
    const hour = new Date().getHours();
    if (hour < 6) return 'Night owl mode';
    if (hour < 12) return 'Rise and shine';
    if (hour < 17) return 'Good afternoon';
    if (hour < 21) return 'Good evening';
    return 'Night owl mode';
}

interface SummaryCard {
    id: string;
    title: string;
    description: string;
    icon: typeof Sun;
    iconBg: string;
    iconColor: string;
    prompt: string;
    isPro?: boolean;
}

const summaryCards: SummaryCard[] = [
    {
        id: 'morning-brief',
        title: 'Morning Brief',
        description: 'Everything to kickstart your day',
        icon: Sun,
        iconBg: 'bg-amber-500/20',
        iconColor: 'text-amber-400',
        prompt: 'Give me a morning brief. Summarize what I worked on yesterday, any pending tasks, and what I should focus on today based on my recent activity patterns.',
    },
    {
        id: 'standup-update',
        title: 'Standup Update',
        description: "What you did, what's next, and blockers",
        icon: CalendarCheck,
        iconBg: 'bg-emerald-500/20',
        iconColor: 'text-emerald-400',
        prompt: 'Generate a standup update for me. What did I do yesterday? What am I working on today? Are there any potential blockers based on my activity?',
    },
    {
        id: 'custom-summary',
        title: 'Custom Summary',
        description: 'Custom time, filters & instructions',
        icon: SlidersHorizontal,
        iconBg: 'bg-purple-500/20',
        iconColor: 'text-purple-400',
        prompt: 'Create a detailed summary of my activity for the past week. Include which applications I used most, how much time I spent on different categories, and any notable patterns.',
        isPro: true,
    },
    {
        id: 'top-of-mind',
        title: "What's Top of Mind",
        description: 'Recurring topics ranked by importance',
        icon: AlertCircle,
        iconBg: 'bg-red-500/20',
        iconColor: 'text-red-400',
        prompt: 'What topics and projects have I been focusing on most this week? Rank them by how much time and attention I gave them.',
    },
    {
        id: 'ai-habits',
        title: 'AI Habits',
        description: 'AI usage patterns and model preferences',
        icon: Brain,
        iconBg: 'bg-violet-500/20',
        iconColor: 'text-violet-400',
        prompt: 'Analyze my AI tool usage patterns. Which AI assistants and models have I been using? How often? What types of tasks do I use them for?',
        isPro: true,
    },
    {
        id: 'discover',
        title: 'Discover',
        description: 'Reminders, Recaps, and More.',
        icon: Compass,
        iconBg: 'bg-teal-500/20',
        iconColor: 'text-teal-400',
        prompt: 'What interesting things did I do recently that I might have forgotten about? Any applications or websites I visited briefly that might be worth revisiting?',
    },
];

export function HomePage({ onNavigate, onChatWithPrompt }: HomePageProps) {
    const [stats, setStats] = useState<ActivityStats | null>(null);

    useEffect(() => {
        const loadStats = async () => {
            try {
                const { start, end } = getDayRange(0);
                const statsData = await getActivityStats(start, end);
                setStats(statsData);
            } catch (error) {
                console.error('Failed to load stats:', error);
            }
        };
        loadStats();
    }, []);

    return (
        <div className="flex flex-col min-h-full">
            <div className="flex-1 overflow-y-auto px-6 lg:px-12 py-8">
                <div className="max-w-3xl mx-auto">
                    {/* Greeting */}
                    <div className="flex items-center gap-4 mb-10">
                        <div className="w-12 h-12 rounded-2xl bg-dark-800 flex items-center justify-center">
                            <span className="text-2xl">💡</span>
                        </div>
                        <h1 className="text-3xl font-bold text-white tracking-tight">
                            {getGreeting()}, User
                        </h1>
                    </div>

                    {/* Single-Click Summaries */}
                    <section className="mb-10">
                        <h2 className="text-xs font-semibold text-dark-400 uppercase tracking-wider mb-4">
                            Single-Click Summaries
                        </h2>
                        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                            {summaryCards.map((card) => (
                                <button
                                    key={card.id}
                                    onClick={() => onChatWithPrompt(card.prompt)}
                                    className="group relative flex flex-col items-start gap-3 p-4 rounded-xl border border-dark-700/60 bg-dark-900 hover:bg-dark-800/80 hover:border-dark-600 transition-all duration-200 text-left"
                                    id={`summary-${card.id}`}
                                >
                                    <div className={`w-10 h-10 rounded-xl ${card.iconBg} flex items-center justify-center`}>
                                        <card.icon className={`w-5 h-5 ${card.iconColor}`} />
                                    </div>
                                    {card.isPro && (
                                        <div className="absolute top-3 right-3 flex items-center gap-1 px-1.5 py-0.5 rounded bg-purple-600/20">
                                            <Lock className="w-2.5 h-2.5 text-purple-400" />
                                        </div>
                                    )}
                                    <div>
                                        <h3 className="text-sm font-semibold text-white group-hover:text-white">
                                            {card.title}
                                        </h3>
                                        <p className="text-xs text-dark-400 mt-0.5 line-clamp-1">
                                            {card.description}
                                        </p>
                                    </div>
                                </button>
                            ))}
                        </div>
                    </section>

                    {/* Freeform Chat */}
                    <section className="mb-10">
                        <div className="flex items-center gap-2 mb-4">
                            <h2 className="text-xs font-semibold text-dark-400 uppercase tracking-wider">
                                Freeform Chat
                            </h2>
                            <RefreshCw className="w-3 h-3 text-dark-500 hover:text-dark-300 cursor-pointer transition-colors" />
                        </div>
                        <button
                            onClick={() => onNavigate('chat')}
                            className="group flex items-center gap-3 w-full sm:w-auto px-5 py-3 rounded-xl border border-dark-700/60 bg-dark-900 hover:bg-dark-800/80 hover:border-dark-600 transition-all duration-200"
                            id="start-new-chat"
                        >
                            <MessageSquare className="w-5 h-5 text-emerald-400" />
                            <span className="text-sm font-medium text-emerald-400">Start New Chat</span>
                            <ArrowRight className="w-4 h-4 text-emerald-400 ml-auto opacity-0 group-hover:opacity-100 transition-opacity" />
                        </button>
                    </section>
                </div>
            </div>

            {/* Bottom Activity Bar */}
            <ActivityTimebar stats={stats} />
        </div>
    );
}
