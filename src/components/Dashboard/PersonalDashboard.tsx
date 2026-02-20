import { useEffect, useState } from 'react';
import { RefreshCw, CalendarClock, FolderKanban, MessageCircle, Sparkles } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Card, CardHeader, CardContent, Button } from '../common';
import { getDashboardOverview, refreshDashboardOverview, summarizeContact, summarizeProject } from '../../services/tauri';
import type { DashboardOverview, DashboardTask, ProjectOverview, ContactOverview } from '../../types';

function formatTime(ts?: number): string {
    if (!ts) return 'N/A';
    return new Date(ts * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

type DetailPopup = {
    title: string;
    summary: string;
    detail?: string;
    when?: string;
};

export function PersonalDashboard() {
    const [data, setData] = useState<DashboardOverview | null>(null);
    const [loading, setLoading] = useState(true);
    const [refreshing, setRefreshing] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [detailPopup, setDetailPopup] = useState<DetailPopup | null>(null);

    const load = async (forceRefresh = false) => {
        try {
            setError(null);
            const next = forceRefresh ? await refreshDashboardOverview() : await getDashboardOverview(false);
            setData(next);
        } catch (e) {
            setError(e instanceof Error ? e.message : 'Failed to load dashboard');
        } finally {
            setLoading(false);
            setRefreshing(false);
        }
    };

    useEffect(() => {
        load(false);
    }, []);

    const openContactDetail = async (item: ContactOverview) => {
        const when = item.last_seen ? formatTime(item.last_seen) : 'Unknown time';
        setDetailPopup({
            title: item.name,
            summary: 'Generating AI summary...',
            detail: `Fetching recent interactions...`,
            when,
        });

        try {
            const summary = await summarizeContact(item.name);
            setDetailPopup({
                title: item.name,
                summary: summary,
                detail: `AI generated summary based on recent interactions.`,
                when,
            });
        } catch (e) {
            setDetailPopup({
                title: item.name,
                summary: item.context || 'Communication activity detected.',
                detail: `Failed to generate AI summary: ${e instanceof Error ? e.message : String(e)}`,
                when,
            });
        }
    };

    const openProjectDetail = async (item: ProjectOverview) => {
        const when = data?.updated_at ? formatTime(data.updated_at) : 'Unknown time';
        setDetailPopup({
            title: item.name,
            summary: 'Generating AI summary...',
            detail: `Fetching recent file changes...`,
            when,
        });

        try {
            const summary = await summarizeProject(item.name);
            setDetailPopup({
                title: item.name,
                summary: summary,
                detail: `AI generated summary based on recent file changes.`,
                when,
            });
        } catch (e) {
            setDetailPopup({
                title: item.name,
                summary: item.update || `${item.files_changed} file change(s) detected today.`,
                detail: `Failed to generate AI summary: ${e instanceof Error ? e.message : String(e)}`,
                when,
            });
        }
    };

    const openDeadlineDetail = (item: DashboardTask) => {
        setDetailPopup({
            title: item.title,
            summary: `Status: ${item.status}`,
            detail: `Source: ${item.source}${item.due_date ? `\nDue hint: ${item.due_date}` : '\nNo due hint detected.'}`,
            when: data?.updated_at ? formatTime(data.updated_at) : 'Unknown time',
        });
    };

    if (loading) {
        return (
            <div className="flex items-center justify-center py-12">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-500" />
            </div>
        );
    }

    return (
        <div className="space-y-6">
            <Card variant="bordered">
                <CardHeader
                    title="Personal Dashboard"
                    subtitle={`Today-only summary${data?.updated_at ? ` - updated ${formatTime(data.updated_at)}` : ''}`}
                    action={
                        <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => {
                                setRefreshing(true);
                                load(true);
                            }}
                            disabled={refreshing}
                        >
                            <RefreshCw className={`w-4 h-4 ${refreshing ? 'animate-spin' : ''}`} />
                            Refresh
                        </Button>
                    }
                />
                <CardContent>
                    {error ? (
                        <p className="text-sm text-red-400">{error}</p>
                    ) : (
                        <div className="text-sm text-dark-200 leading-relaxed whitespace-pre-wrap break-words overflow-hidden [&>p]:mb-4 [&>h3]:text-white [&>h3]:font-semibold [&>h3]:mb-2 [&>ul]:list-disc [&>ul]:pl-5 [&>ul]:mb-4 [&>ol]:list-decimal [&>ol]:pl-5 [&>ol]:mb-4 [&_strong]:text-white">
                            <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                {data?.summary || 'No summary available yet.'}
                            </ReactMarkdown>
                        </div>
                    )}
                    {data?.focus_points && data.focus_points.length > 0 && (
                        <div className="mt-4 space-y-2">
                            {data.focus_points.slice(0, 5).map((point) => (
                                <div key={point} className="flex items-start gap-2 text-sm text-dark-300">
                                    <Sparkles className="w-3.5 h-3.5 text-primary-400 mt-0.5 flex-shrink-0" />
                                    <span>{point}</span>
                                </div>
                            ))}
                        </div>
                    )}
                </CardContent>
            </Card>

            <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
                <Card variant="bordered">
                    <CardHeader title="Deadlines" subtitle="Assignments and due items" />
                    <CardContent>
                        <div className="space-y-2">
                            {data?.deadlines?.length ? data.deadlines.slice(0, 8).map((item, idx) => (
                                <button
                                    key={`${item.title}-${idx}`}
                                    onClick={() => openDeadlineDetail(item)}
                                    className="w-full text-left p-3 bg-dark-800 rounded-lg hover:bg-dark-700 transition-colors"
                                >
                                    <div className="flex items-center gap-2 mb-1">
                                        <CalendarClock className="w-4 h-4 text-amber-400" />
                                        <p className="text-sm text-white truncate">{item.title}</p>
                                    </div>
                                    <p className="text-xs text-dark-400">
                                        {item.due_date || 'No due date detected'} - {item.status}
                                    </p>
                                </button>
                            )) : (
                                <p className="text-xs text-dark-500">No active deadlines detected.</p>
                            )}
                        </div>
                    </CardContent>
                </Card>

                <Card variant="bordered">
                    <CardHeader title="Projects" subtitle="Overview from code/file changes" />
                    <CardContent>
                        <div className="space-y-2">
                            {data?.projects?.length ? data.projects.slice(0, 8).map((item, idx) => (
                                <button
                                    key={`${item.name}-${idx}`}
                                    onClick={() => openProjectDetail(item)}
                                    className="w-full text-left p-3 bg-dark-800 rounded-lg hover:bg-dark-700 transition-colors"
                                >
                                    <div className="flex items-center gap-2 mb-1">
                                        <FolderKanban className="w-4 h-4 text-blue-400" />
                                        <p className="text-sm text-white truncate">{item.name}</p>
                                    </div>
                                    <div className="text-xs text-dark-400 whitespace-pre-wrap break-all max-h-40 overflow-y-auto pr-1 [&>p]:mb-2 [&>ul]:list-disc [&>ul]:pl-4 [&>ol]:list-decimal [&>ol]:pl-4 [&_strong]:text-white">
                                        <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                            {item.update}
                                        </ReactMarkdown>
                                    </div>
                                </button>
                            )) : (
                                <p className="text-xs text-dark-500">No project activity detected yet.</p>
                            )}
                        </div>
                    </CardContent>
                </Card>

                <Card variant="bordered">
                    <CardHeader title="Who You Texted" subtitle="Detected from communication context" />
                    <CardContent>
                        <div className="space-y-2">
                            {data?.contacts?.length ? data.contacts.slice(0, 8).map((item, idx) => (
                                <button
                                    key={`${item.name}-${idx}`}
                                    onClick={() => openContactDetail(item)}
                                    className="w-full text-left p-3 bg-dark-800 rounded-lg hover:bg-dark-700 transition-colors"
                                >
                                    <div className="flex items-center gap-2 mb-1">
                                        <MessageCircle className="w-4 h-4 text-emerald-400" />
                                        <p className="text-sm text-white truncate">{item.name}</p>
                                    </div>
                                    <p className="text-xs text-dark-400 break-words">
                                        {item.context || 'No message context'}{item.last_seen ? ` - ${formatTime(item.last_seen)}` : ''}
                                    </p>
                                </button>
                            )) : (
                                <p className="text-xs text-dark-500">No communication contacts detected yet.</p>
                            )}
                        </div>
                    </CardContent>
                </Card>
            </div>

            <AnimatePresence>
                {detailPopup && (
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center px-4"
                        onClick={() => setDetailPopup(null)}
                    >
                        <motion.div
                            initial={{ scale: 0.95, opacity: 0 }}
                            animate={{ scale: 1, opacity: 1 }}
                            exit={{ scale: 0.95, opacity: 0 }}
                            transition={{ type: "spring", duration: 0.3 }}
                            className="w-full max-w-md bg-dark-900 border border-dark-700 rounded-xl p-4 shadow-2xl"
                            onClick={(e) => e.stopPropagation()}
                        >
                            <div className="flex items-center justify-between mb-3">
                                <h3 className="text-sm font-semibold text-white truncate pr-4">{detailPopup.title}</h3>
                                <button
                                    onClick={() => setDetailPopup(null)}
                                    className="text-xs text-dark-400 hover:text-white"
                                >
                                    Close
                                </button>
                            </div>
                            <div className="text-sm text-dark-200 leading-relaxed max-h-[60vh] overflow-y-auto pr-2 [&>p]:mb-4 [&>h3]:text-white [&>h3]:font-semibold [&>h3]:mb-2 [&>ul]:list-disc [&>ul]:pl-5 [&>ul]:mb-4 [&>ol]:list-decimal [&>ol]:pl-5 [&>ol]:mb-4 [&_strong]:text-white">
                                <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                    {detailPopup.summary}
                                </ReactMarkdown>
                            </div>
                            {detailPopup.detail && (
                                <p className="mt-3 text-xs text-dark-400 whitespace-pre-wrap leading-relaxed">
                                    {detailPopup.detail}
                                </p>
                            )}
                            {detailPopup.when && (
                                <p className="mt-3 text-[11px] text-dark-500">Last seen/updated: {detailPopup.when}</p>
                            )}
                        </motion.div>
                    </motion.div>
                )}
            </AnimatePresence>
        </div>
    );
}
