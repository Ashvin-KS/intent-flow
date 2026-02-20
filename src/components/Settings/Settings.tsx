import { useState, useEffect, useRef } from 'react';
import {
    Settings as SettingsIcon,
    Globe,
    Eye,
    Database,
    Brain,
    Shield,
    Bell,
    Save,
    RotateCcw,
    Loader2,
    RefreshCw,
    Trash2,
    Download,
    CheckCircle2,
    ChevronDown,
    Star,
    X,
} from 'lucide-react';
import { Card, CardHeader, CardContent, Button } from '../common';
import { useSettings } from '../../hooks/useSettings';
import { getStorageStats, cleanupOldData, exportData, getNvidiaModels, ModelInfo } from '../../services/tauri';
import type { Settings as SettingsType, StorageStats } from '../../types';
import { formatBytes } from '../../lib/utils';
import { useFavoriteModels } from '../../hooks/useFavoriteModels';

type SettingsTab = 'general' | 'tracking' | 'storage' | 'ai' | 'privacy' | 'notifications';

export function SettingsPanel() {
    const { settings, isLoading, isSaving, error, updateSettings } = useSettings();
    const { favorites, removeFavorite } = useFavoriteModels();
    const [activeTab, setActiveTab] = useState<SettingsTab>('general');
    const [localSettings, setLocalSettings] = useState<SettingsType | null>(null);
    const [storageStats, setStorageStats] = useState<StorageStats | null>(null);
    const [saveSuccess, setSaveSuccess] = useState(false);
    const [actionMessage, setActionMessage] = useState<string | null>(null);
    const [availableModels, setAvailableModels] = useState<ModelInfo[]>([]);
    const [isLoadingModels, setIsLoadingModels] = useState(false);
    const [modelsError, setModelsError] = useState<string | null>(null);
    const [modelSearch, setModelSearch] = useState('');
    const [showModelDropdown, setShowModelDropdown] = useState(false);
    const modelInputRef = useRef<HTMLInputElement>(null);
    const modelDropdownRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (settings) {
            setLocalSettings(settings);
        }
    }, [settings]);

    useEffect(() => {
        loadStorageStats();
    }, []);

    // Fetch models when AI tab is active and we have an API key
    useEffect(() => {
        if (activeTab === 'ai' && localSettings?.ai.api_key) {
            loadModels(localSettings.ai.api_key);
        }
    }, [activeTab, localSettings?.ai.api_key]);

    // Close dropdown when clicking outside
    useEffect(() => {
        const handleClickOutside = (event: MouseEvent) => {
            if (modelDropdownRef.current && !modelDropdownRef.current.contains(event.target as Node)) {
                setShowModelDropdown(false);
            }
        };
        document.addEventListener('mousedown', handleClickOutside);
        return () => document.removeEventListener('mousedown', handleClickOutside);
    }, []);

    const loadModels = async (apiKey: string) => {
        if (!apiKey || apiKey.length < 10) return;
        setIsLoadingModels(true);
        setModelsError(null);
        try {
            const models = await getNvidiaModels(apiKey);
            setAvailableModels(models);
        } catch (e) {
            console.error('Failed to load models:', e);
            setModelsError('Failed to fetch models. Check your API key.');
        } finally {
            setIsLoadingModels(false);
        }
    };

    const loadStorageStats = async () => {
        try {
            const stats = await getStorageStats();
            setStorageStats(stats);
        } catch (e) {
            console.error('Failed to load storage stats:', e);
        }
    };

    // Auto-save when AI settings change (debounced)
    useEffect(() => {
        if (!localSettings || !settings) return;

        // Only auto-save AI settings changes (not first load)
        const aiChanged = localSettings.ai.model !== settings.ai.model ||
            localSettings.ai.api_key !== settings.ai.api_key ||
            localSettings.ai.enabled !== settings.ai.enabled;

        if (aiChanged && !isLoading) {
            const timer = setTimeout(async () => {
                try {
                    await updateSettings(localSettings);
                    setSaveSuccess(true);
                    setTimeout(() => setSaveSuccess(false), 2000);
                } catch (e) {
                    console.error('Auto-save failed:', e);
                }
            }, 1000); // Auto-save after 1 second of no changes

            return () => clearTimeout(timer);
        }
    }, [localSettings?.ai, settings?.ai, isLoading]);

    const handleSave = async () => {
        if (!localSettings) return;
        try {
            await updateSettings(localSettings);
            setSaveSuccess(true);
            setTimeout(() => setSaveSuccess(false), 2000);
        } catch (e) {
            console.error('Failed to save settings:', e);
        }
    };

    const handleReset = () => {
        if (settings) setLocalSettings(settings);
    };

    const handleCleanup = async () => {
        try {
            const days = localSettings?.storage.retention_days || 365;
            const count = await cleanupOldData(days);
            setActionMessage(`Cleaned up ${count} old records`);
            await loadStorageStats();
            setTimeout(() => setActionMessage(null), 3000);
        } catch (e) {
            setActionMessage('Failed to cleanup data');
        }
    };

    const handleExport = async () => {
        try {
            const path = await exportData();
            setActionMessage(`Data exported to ${path}`);
            setTimeout(() => setActionMessage(null), 5000);
        } catch (e) {
            setActionMessage('Failed to export data');
        }
    };

    if (isLoading || !localSettings) {
        return (
            <div className="flex items-center justify-center py-12">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-500" />
            </div>
        );
    }

    const tabs = [
        { id: 'general' as const, label: 'General', icon: Globe },
        { id: 'tracking' as const, label: 'Tracking', icon: Eye },
        { id: 'storage' as const, label: 'Storage', icon: Database },
        { id: 'ai' as const, label: 'AI', icon: Brain },
        { id: 'privacy' as const, label: 'Privacy', icon: Shield },
        { id: 'notifications' as const, label: 'Notifications', icon: Bell },
    ];

    const update = (section: string, field: string, value: any) => {
        setLocalSettings((prev) => {
            if (!prev) return prev;
            return {
                ...prev,
                [section]: {
                    ...(prev as any)[section],
                    [field]: value,
                },
            };
        });
    };

    return (
        <div className="space-y-6">
            {/* Action bar */}
            <Card variant="bordered">
                <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                        <SettingsIcon className="w-5 h-5 text-primary-400" />
                        <h2 className="text-lg font-semibold text-white">Settings</h2>
                    </div>
                    <div className="flex items-center gap-2">
                        {saveSuccess && (
                            <span className="flex items-center gap-1 text-sm text-green-400">
                                <CheckCircle2 className="w-4 h-4" /> Saved
                            </span>
                        )}
                        <Button variant="secondary" size="sm" onClick={handleReset}>
                            <RotateCcw className="w-4 h-4" /> Reset
                        </Button>
                        <Button variant="primary" size="sm" onClick={handleSave} isLoading={isSaving}>
                            <Save className="w-4 h-4" /> Save
                        </Button>
                    </div>
                </div>
            </Card>

            {/* Tabs + Content */}
            <div className="flex gap-6">
                {/* Sidebar tabs */}
                <div className="w-48 flex-shrink-0 space-y-1">
                    {tabs.map((tab) => (
                        <button
                            key={tab.id}
                            onClick={() => setActiveTab(tab.id)}
                            className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors ${activeTab === tab.id
                                ? 'bg-primary-600/20 text-primary-400'
                                : 'text-dark-400 hover:text-white hover:bg-dark-800'
                                }`}
                        >
                            <tab.icon className="w-4 h-4" />
                            {tab.label}
                        </button>
                    ))}
                </div>

                {/* Content */}
                <div className="flex-1">
                    {activeTab === 'general' && (
                        <Card variant="bordered">
                            <CardHeader title="General" subtitle="App appearance and behavior" />
                            <CardContent>
                                <div className="space-y-5">
                                    <SettingSelect
                                        label="Theme"
                                        value={localSettings.general.theme}
                                        onChange={(v) => update('general', 'theme', v)}
                                        options={[
                                            { value: 'dark', label: 'Dark' },
                                            { value: 'light', label: 'Light' },
                                            { value: 'system', label: 'System' },
                                        ]}
                                    />
                                    <SettingSelect
                                        label="Startup Behavior"
                                        value={localSettings.general.startup_behavior}
                                        onChange={(v) => update('general', 'startup_behavior', v)}
                                        options={[
                                            { value: 'show_window', label: 'Show Window' },
                                            { value: 'minimized_to_tray', label: 'Minimize to Tray' },
                                            { value: 'hidden', label: 'Hidden' },
                                        ]}
                                    />
                                    <SettingToggle
                                        label="Minimize to Tray"
                                        description="Minimize to system tray instead of taskbar"
                                        value={localSettings.general.minimize_to_tray}
                                        onChange={(v) => update('general', 'minimize_to_tray', v)}
                                    />
                                    <SettingToggle
                                        label="Close to Tray"
                                        description="Close window to tray instead of quitting"
                                        value={localSettings.general.close_to_tray}
                                        onChange={(v) => update('general', 'close_to_tray', v)}
                                    />
                                </div>
                            </CardContent>
                        </Card>
                    )}

                    {activeTab === 'tracking' && (
                        <Card variant="bordered">
                            <CardHeader title="Tracking" subtitle="Activity tracking configuration" />
                            <CardContent>
                                <div className="space-y-5">
                                    <SettingToggle
                                        label="Enable Tracking"
                                        description="Track your application usage"
                                        value={localSettings.tracking.enabled}
                                        onChange={(v) => update('tracking', 'enabled', v)}
                                    />
                                    <SettingNumber
                                        label="Tracking Interval (seconds)"
                                        value={localSettings.tracking.tracking_interval}
                                        onChange={(v) => update('tracking', 'tracking_interval', v)}
                                        min={1}
                                        max={60}
                                    />
                                    <SettingNumber
                                        label="Idle Timeout (seconds)"
                                        value={localSettings.tracking.idle_timeout}
                                        onChange={(v) => update('tracking', 'idle_timeout', v)}
                                        min={30}
                                        max={3600}
                                    />
                                    <SettingToggle
                                        label="Track Browser Activity"
                                        description="Track browser tab titles and URLs"
                                        value={localSettings.tracking.track_browser}
                                        onChange={(v) => update('tracking', 'track_browser', v)}
                                    />
                                </div>
                            </CardContent>
                        </Card>
                    )}

                    {activeTab === 'storage' && (
                        <Card variant="bordered">
                            <CardHeader title="Storage" subtitle="Data management and retention" />
                            <CardContent>
                                <div className="space-y-5">
                                    {/* Storage stats */}
                                    {storageStats && (
                                        <div className="p-4 bg-dark-800 rounded-lg border border-dark-700">
                                            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                                                <div>
                                                    <p className="text-xs text-dark-400">Database Size</p>
                                                    <p className="text-sm font-bold text-white">{formatBytes(storageStats.total_size_bytes)}</p>
                                                </div>
                                                <div>
                                                    <p className="text-xs text-dark-400">Activities</p>
                                                    <p className="text-sm font-bold text-white">{storageStats.activities_count.toLocaleString()}</p>
                                                </div>
                                                <div>
                                                    <p className="text-xs text-dark-400">Patterns</p>
                                                    <p className="text-sm font-bold text-white">{storageStats.patterns_count.toLocaleString()}</p>
                                                </div>
                                                <div>
                                                    <p className="text-xs text-dark-400">Entries</p>
                                                    <p className="text-sm font-bold text-white">{storageStats.entries_count.toLocaleString()}</p>
                                                </div>
                                            </div>
                                        </div>
                                    )}

                                    <SettingNumber
                                        label="Retention Days"
                                        value={localSettings.storage.retention_days}
                                        onChange={(v) => update('storage', 'retention_days', v)}
                                        min={7}
                                        max={3650}
                                    />
                                    <SettingToggle
                                        label="Auto Cleanup"
                                        description="Automatically remove data older than retention period"
                                        value={localSettings.storage.auto_cleanup}
                                        onChange={(v) => update('storage', 'auto_cleanup', v)}
                                    />
                                    <SettingToggle
                                        label="Compression"
                                        description="Compress stored data to save space"
                                        value={localSettings.storage.compression_enabled}
                                        onChange={(v) => update('storage', 'compression_enabled', v)}
                                    />

                                    {/* Actions */}
                                    <div className="flex gap-3 pt-2">
                                        <Button variant="secondary" size="sm" onClick={handleCleanup}>
                                            <Trash2 className="w-4 h-4" /> Cleanup Old Data
                                        </Button>
                                        <Button variant="secondary" size="sm" onClick={handleExport}>
                                            <Download className="w-4 h-4" /> Export Data
                                        </Button>
                                    </div>

                                    {actionMessage && (
                                        <p className="text-sm text-green-400">{actionMessage}</p>
                                    )}
                                </div>
                            </CardContent>
                        </Card>
                    )}

                    {activeTab === 'ai' && (
                        <Card variant="bordered">
                            <CardHeader title="AI Configuration" subtitle="AI provider settings for intent parsing" />
                            <CardContent>
                                <div className="space-y-5">
                                    <SettingToggle
                                        label="Enable AI"
                                        description="Use AI for enhanced intent parsing and pattern recognition"
                                        value={localSettings.ai.enabled}
                                        onChange={(v) => update('ai', 'enabled', v)}
                                    />
                                    <SettingSelect
                                        label="Provider"
                                        value={localSettings.ai.provider}
                                        onChange={(v) => update('ai', 'provider', v)}
                                        options={[
                                            { value: 'nvidia', label: 'NVIDIA NIM' },
                                            { value: 'openai', label: 'OpenAI' },
                                            { value: 'anthropic', label: 'Anthropic' },
                                            { value: 'local', label: 'Local Model' },
                                        ]}
                                    />
                                    <SettingText
                                        label="API Key"
                                        value={localSettings.ai.api_key}
                                        onChange={(v) => update('ai', 'api_key', v)}
                                        placeholder="nvapi-..."
                                        type="password"
                                    />
                                    <div>
                                        <div className="flex items-center justify-between mb-1">
                                            <label className="block text-sm font-medium text-white">Model</label>
                                            <button
                                                type="button"
                                                onClick={() => localSettings?.ai.api_key && loadModels(localSettings.ai.api_key)}
                                                disabled={isLoadingModels || !localSettings?.ai.api_key}
                                                className="text-xs text-primary-400 hover:text-primary-300 disabled:opacity-50 flex items-center gap-1"
                                            >
                                                {isLoadingModels ? (
                                                    <Loader2 className="w-3 h-3 animate-spin" />
                                                ) : (
                                                    <RefreshCw className="w-3 h-3" />
                                                )}
                                                Refresh
                                            </button>
                                        </div>
                                        <div className="relative" ref={modelDropdownRef}>
                                            <div className="relative">
                                                <input
                                                    ref={modelInputRef}
                                                    type="text"
                                                    value={availableModels.length > 0 ? modelSearch || localSettings.ai.model : localSettings.ai.model}
                                                    onChange={(e) => {
                                                        const value = e.target.value;
                                                        if (availableModels.length > 0) {
                                                            setModelSearch(value);
                                                            setShowModelDropdown(true);
                                                        }
                                                        update('ai', 'model', value);
                                                    }}
                                                    onFocus={() => {
                                                        if (availableModels.length > 0) {
                                                            setShowModelDropdown(true);
                                                            setModelSearch('');
                                                        }
                                                    }}
                                                    placeholder={availableModels.length > 0 ? "Search models..." : "moonshotai/kimi-k2.5"}
                                                    className="w-full px-3 py-2 pr-8 bg-dark-800 border border-dark-700 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-primary-500"
                                                />
                                                {availableModels.length > 0 && (
                                                    <ChevronDown className="absolute right-2 top-1/2 -translate-y-1/2 w-4 h-4 text-dark-400 pointer-events-none" />
                                                )}
                                            </div>
                                            {showModelDropdown && availableModels.length > 0 && (
                                                <div className="absolute z-10 w-full mt-1 max-h-60 overflow-auto bg-dark-800 border border-dark-700 rounded-lg shadow-lg">
                                                    {availableModels
                                                        .filter((model) =>
                                                            modelSearch === '' ||
                                                            model.id.toLowerCase().includes(modelSearch.toLowerCase()) ||
                                                            model.name.toLowerCase().includes(modelSearch.toLowerCase())
                                                        )
                                                        .map((model) => (
                                                            <button
                                                                key={model.id}
                                                                type="button"
                                                                onClick={() => {
                                                                    update('ai', 'model', model.id);
                                                                    setModelSearch('');
                                                                    setShowModelDropdown(false);
                                                                }}
                                                                className={`w-full px-3 py-2 text-left text-sm hover:bg-dark-700 transition-colors ${localSettings.ai.model === model.id
                                                                        ? 'text-primary-400 bg-dark-700/50'
                                                                        : 'text-white'
                                                                    }`}
                                                            >
                                                                {model.name}
                                                            </button>
                                                        ))}
                                                    {availableModels.filter((model) =>
                                                        modelSearch === '' ||
                                                        model.id.toLowerCase().includes(modelSearch.toLowerCase()) ||
                                                        model.name.toLowerCase().includes(modelSearch.toLowerCase())
                                                    ).length === 0 && (
                                                            <div className="px-3 py-2 text-sm text-dark-400">No models found</div>
                                                        )}
                                                </div>
                                            )}
                                        </div>
                                        {modelsError && (
                                            <p className="text-xs text-red-400 mt-1">{modelsError}</p>
                                        )}
                                        {isLoadingModels && (
                                            <p className="text-xs text-dark-400 mt-1">Loading models...</p>
                                        )}
                                    </div>
                                    <SettingToggle
                                        label="Local Only"
                                        description="Only use local processing, no API calls"
                                        value={localSettings.ai.local_only}
                                        onChange={(v) => update('ai', 'local_only', v)}
                                    />
                                    <SettingToggle
                                        label="Fallback to Local"
                                        description="Use local processing when API is unavailable"
                                        value={localSettings.ai.fallback_to_local}
                                        onChange={(v) => update('ai', 'fallback_to_local', v)}
                                    />

                                    {/* Recent Models */}
                                    <div className="border-t border-dark-700/50 pt-5">
                                        <div className="flex items-center justify-between mb-3">
                                            <div>
                                                <label className="block text-sm font-medium text-white">Recent Models</label>
                                                <p className="text-xs text-dark-400 mt-0.5">Last 5 models used in Chat (auto-updated)</p>
                                            </div>
                                            <span className="text-xs text-dark-500">{favorites.length}/5</span>
                                        </div>

                                        {/* Current recent models */}
                                        {favorites.length > 0 && (
                                            <div className="space-y-1.5 mb-3">
                                                {favorites.map((fav) => (
                                                    <div key={fav.id} className="flex items-center gap-2 px-3 py-2 bg-dark-800 rounded-lg">
                                                        <Star className="w-3.5 h-3.5 text-amber-400 fill-amber-400 flex-shrink-0" />
                                                        <span className="text-xs text-white flex-1 truncate">{fav.name}</span>
                                                        <button
                                                            onClick={() => removeFavorite(fav.id)}
                                                            className="text-dark-500 hover:text-red-400 transition-colors flex-shrink-0"
                                                            title="Remove from recent list"
                                                        >
                                                            <X className="w-3.5 h-3.5" />
                                                        </button>
                                                    </div>
                                                ))}
                                            </div>
                                        )}

                                        {favorites.length === 0 && (
                                            <p className="text-xs text-dark-500 italic">Use a model in Chat to populate this list</p>
                                        )}
                                    </div>
                                </div>
                            </CardContent>
                        </Card>
                    )}

                    {activeTab === 'privacy' && (
                        <Card variant="bordered">
                            <CardHeader title="Privacy" subtitle="Control how your data is handled" />
                            <CardContent>
                                <div className="space-y-5">
                                    <SettingToggle
                                        label="Encrypt Database"
                                        description="Encrypt the local database for added security"
                                        value={localSettings.privacy.encrypt_database}
                                        onChange={(v) => update('privacy', 'encrypt_database', v)}
                                    />
                                    <SettingToggle
                                        label="Exclude Incognito"
                                        description="Don't track incognito browser activity"
                                        value={localSettings.privacy.exclude_incognito}
                                        onChange={(v) => update('privacy', 'exclude_incognito', v)}
                                    />
                                    <SettingToggle
                                        label="Anonymize Data"
                                        description="Anonymize sensitive data in exports and logs"
                                        value={localSettings.privacy.anonymize_data}
                                        onChange={(v) => update('privacy', 'anonymize_data', v)}
                                    />
                                </div>
                            </CardContent>
                        </Card>
                    )}

                    {activeTab === 'notifications' && (
                        <Card variant="bordered">
                            <CardHeader title="Notifications" subtitle="Manage notification preferences" />
                            <CardContent>
                                <div className="space-y-5">
                                    <SettingToggle
                                        label="Workflow Suggestions"
                                        description="Get notified about relevant workflow suggestions"
                                        value={localSettings.notifications.workflow_suggestions}
                                        onChange={(v) => update('notifications', 'workflow_suggestions', v)}
                                    />
                                    <SettingToggle
                                        label="Pattern Insights"
                                        description="Receive insights about your activity patterns"
                                        value={localSettings.notifications.pattern_insights}
                                        onChange={(v) => update('notifications', 'pattern_insights', v)}
                                    />
                                    <SettingToggle
                                        label="Daily Summary"
                                        description="Get a daily summary of your activities"
                                        value={localSettings.notifications.daily_summary}
                                        onChange={(v) => update('notifications', 'daily_summary', v)}
                                    />
                                    <SettingText
                                        label="Summary Time"
                                        value={localSettings.notifications.summary_time}
                                        onChange={(v) => update('notifications', 'summary_time', v)}
                                        placeholder="09:00"
                                    />
                                </div>
                            </CardContent>
                        </Card>
                    )}
                </div>
            </div>

            {error && (
                <p className="text-sm text-red-400 text-center">{error}</p>
            )}
        </div>
    );
}

/* ─────────────── Reusable Setting Field Components ─────────────── */

function SettingToggle({
    label,
    description,
    value,
    onChange,
}: {
    label: string;
    description?: string;
    value: boolean;
    onChange: (v: boolean) => void;
}) {
    return (
        <div className="flex items-center justify-between">
            <div>
                <p className="text-sm font-medium text-white">{label}</p>
                {description && <p className="text-xs text-dark-400 mt-0.5">{description}</p>}
            </div>
            <button
                onClick={() => onChange(!value)}
                className={`relative w-11 h-6 rounded-full transition-colors ${value ? 'bg-primary-600' : 'bg-dark-600'
                    }`}
            >
                <div
                    className={`absolute top-0.5 w-5 h-5 bg-white rounded-full transition-transform ${value ? 'left-[22px]' : 'left-0.5'
                        }`}
                />
            </button>
        </div>
    );
}

function SettingSelect({
    label,
    value,
    onChange,
    options,
}: {
    label: string;
    value: string;
    onChange: (v: string) => void;
    options: { value: string; label: string }[];
}) {
    return (
        <div>
            <label className="block text-sm font-medium text-white mb-1">{label}</label>
            <select
                value={value}
                onChange={(e) => onChange(e.target.value)}
                className="w-full px-3 py-2 bg-dark-800 border border-dark-700 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-primary-500"
            >
                {options.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                        {opt.label}
                    </option>
                ))}
            </select>
        </div>
    );
}

function SettingNumber({
    label,
    value,
    onChange,
    min,
    max,
}: {
    label: string;
    value: number;
    onChange: (v: number) => void;
    min?: number;
    max?: number;
}) {
    return (
        <div>
            <label className="block text-sm font-medium text-white mb-1">{label}</label>
            <input
                type="number"
                value={value}
                onChange={(e) => onChange(parseInt(e.target.value) || 0)}
                min={min}
                max={max}
                className="w-full px-3 py-2 bg-dark-800 border border-dark-700 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-primary-500"
            />
        </div>
    );
}

function SettingText({
    label,
    value,
    onChange,
    placeholder,
    type = 'text',
}: {
    label: string;
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
    type?: string;
}) {
    return (
        <div>
            <label className="block text-sm font-medium text-white mb-1">{label}</label>
            <input
                type={type}
                value={value}
                onChange={(e) => onChange(e.target.value)}
                placeholder={placeholder}
                className="w-full px-3 py-2 bg-dark-800 border border-dark-700 rounded-lg text-white placeholder-dark-400 text-sm focus:outline-none focus:ring-2 focus:ring-primary-500"
            />
        </div>
    );
}

