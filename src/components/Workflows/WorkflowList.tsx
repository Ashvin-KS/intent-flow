import { useState, type FormEvent, type Dispatch, type SetStateAction, type ReactNode } from 'react';
import {
    Zap,
    Plus,
    Play,
    Pencil,
    Trash2,
    Globe,
    FileText,
    Monitor,
    X,
} from 'lucide-react';
import { Card, CardHeader, CardContent, Button, Modal } from '../common';
import { useWorkflows } from '../../hooks/useWorkflows';
import type { Workflow } from '../../types';
import { getRelativeTime } from '../../lib/utils';

export function WorkflowList() {
    const { workflows, suggestions, isLoading, error: _error, create, update, remove, execute } = useWorkflows();
    const [showEditor, setShowEditor] = useState(false);
    const [editingWorkflow, setEditingWorkflow] = useState<Workflow | null>(null);
    const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

    const handleCreate = () => {
        setEditingWorkflow(null);
        setShowEditor(true);
    };

    const handleEdit = (workflow: Workflow) => {
        setEditingWorkflow(workflow);
        setShowEditor(true);
    };

    const handleSave = async (data: { name: string; description: string; icon: string; apps: { path: string; args: string[] }[]; urls: string[]; files: string[] }) => {
        if (editingWorkflow) {
            await update({ ...editingWorkflow, ...data });
        } else {
            await create(data);
        }
        setShowEditor(false);
        setEditingWorkflow(null);
    };

    const handleDelete = async (id: string) => {
        await remove(id);
        setConfirmDelete(null);
    };

    const handleExecute = async (id: string) => {
        await execute(id);
    };

    if (isLoading) {
        return (
            <div className="flex items-center justify-center py-12">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-500" />
            </div>
        );
    }

    return (
        <div className="space-y-6">
            {/* Suggestions */}
            {suggestions.length > 0 && (
                <Card variant="bordered">
                    <CardHeader title="Suggested Workflows" subtitle="Based on your usage patterns" />
                    <CardContent>
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                            {suggestions.map((s) => (
                                <button
                                    key={s.workflow.id}
                                    onClick={() => handleExecute(s.workflow.id)}
                                    className="flex items-center gap-3 p-3 bg-dark-800 rounded-lg hover:bg-dark-700 transition-colors text-left"
                                >
                                    <div className="p-2 bg-primary-600/20 rounded-lg">
                                        <Zap className="w-5 h-5 text-primary-400" />
                                    </div>
                                    <div className="flex-1 min-w-0">
                                        <p className="text-white font-medium truncate">{s.workflow.name}</p>
                                        <p className="text-xs text-dark-400 truncate">{s.reason}</p>
                                    </div>
                                    <span className="text-xs text-dark-500">
                                        {Math.round(s.relevance_score * 100)}%
                                    </span>
                                </button>
                            ))}
                        </div>
                    </CardContent>
                </Card>
            )}

            {/* Workflows List */}
            <Card variant="bordered">
                <CardHeader
                    title="My Workflows"
                    subtitle={`${workflows.length} workflows`}
                    action={
                        <Button variant="primary" size="sm" onClick={handleCreate}>
                            <Plus className="w-4 h-4" />
                            Create
                        </Button>
                    }
                />
                <CardContent>
                    {workflows.length === 0 ? (
                        <div className="text-center py-12 text-dark-400">
                            <Zap className="w-12 h-12 mx-auto mb-4 opacity-50" />
                            <p className="text-lg">No workflows yet</p>
                            <p className="text-sm mt-1">Create your first workflow to automate your work setup</p>
                            <Button variant="primary" size="sm" className="mt-4" onClick={handleCreate}>
                                <Plus className="w-4 h-4" />
                                Create Workflow
                            </Button>
                        </div>
                    ) : (
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                            {workflows.map((workflow) => (
                                <div
                                    key={workflow.id}
                                    className="p-4 bg-dark-800 rounded-xl border border-dark-700 hover:border-dark-600 transition-colors"
                                >
                                    <div className="flex items-start justify-between mb-3">
                                        <div className="flex items-center gap-3">
                                            <div className="text-2xl">{workflow.icon || '⚡'}</div>
                                            <div>
                                                <h4 className="text-white font-semibold">{workflow.name}</h4>
                                                <p className="text-xs text-dark-400 mt-0.5">
                                                    {workflow.description || 'No description'}
                                                </p>
                                            </div>
                                        </div>
                                    </div>

                                    {/* Workflow contents summary */}
                                    <div className="flex flex-wrap gap-2 mb-3">
                                        {workflow.apps.length > 0 && (
                                            <span className="inline-flex items-center gap-1 px-2 py-0.5 text-xs bg-blue-500/10 text-blue-400 rounded">
                                                <Monitor className="w-3 h-3" />
                                                {workflow.apps.length} app{workflow.apps.length > 1 ? 's' : ''}
                                            </span>
                                        )}
                                        {workflow.urls.length > 0 && (
                                            <span className="inline-flex items-center gap-1 px-2 py-0.5 text-xs bg-green-500/10 text-green-400 rounded">
                                                <Globe className="w-3 h-3" />
                                                {workflow.urls.length} URL{workflow.urls.length > 1 ? 's' : ''}
                                            </span>
                                        )}
                                        {workflow.files.length > 0 && (
                                            <span className="inline-flex items-center gap-1 px-2 py-0.5 text-xs bg-amber-500/10 text-amber-400 rounded">
                                                <FileText className="w-3 h-3" />
                                                {workflow.files.length} file{workflow.files.length > 1 ? 's' : ''}
                                            </span>
                                        )}
                                    </div>

                                    <div className="flex items-center justify-between">
                                        <span className="text-xs text-dark-500">
                                            {workflow.use_count > 0
                                                ? `Used ${workflow.use_count}x • ${workflow.last_used ? getRelativeTime(workflow.last_used) : 'never'}`
                                                : 'Never used'}
                                        </span>
                                        <div className="flex items-center gap-1">
                                            <Button variant="ghost" size="sm" onClick={() => handleExecute(workflow.id)}>
                                                <Play className="w-4 h-4 text-green-400" />
                                            </Button>
                                            <Button variant="ghost" size="sm" onClick={() => handleEdit(workflow)}>
                                                <Pencil className="w-4 h-4" />
                                            </Button>
                                            <Button variant="ghost" size="sm" onClick={() => setConfirmDelete(workflow.id)}>
                                                <Trash2 className="w-4 h-4 text-red-400" />
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                            ))}
                        </div>
                    )}
                </CardContent>
            </Card>

            {/* Delete Confirmation Modal */}
            <Modal
                isOpen={confirmDelete !== null}
                onClose={() => setConfirmDelete(null)}
                title="Delete Workflow"
                size="sm"
                footer={
                    <>
                        <Button variant="secondary" onClick={() => setConfirmDelete(null)}>
                            Cancel
                        </Button>
                        <Button variant="danger" onClick={() => confirmDelete && handleDelete(confirmDelete)}>
                            Delete
                        </Button>
                    </>
                }
            >
                <p className="text-dark-300">
                    Are you sure you want to delete this workflow? This action cannot be undone.
                </p>
            </Modal>

            {/* Editor Modal */}
            {showEditor && (
                <WorkflowEditor
                    workflow={editingWorkflow}
                    onSave={handleSave}
                    onClose={() => {
                        setShowEditor(false);
                        setEditingWorkflow(null);
                    }}
                />
            )}
        </div>
    );
}

/* ─────────────────────────── Workflow Editor Modal ─────────────────────────── */

interface WorkflowEditorProps {
    workflow: Workflow | null;
    onSave: (data: {
        name: string;
        description: string;
        icon: string;
        apps: { path: string; args: string[] }[];
        urls: string[];
        files: string[];
    }) => Promise<void>;
    onClose: () => void;
}

function WorkflowEditor({ workflow, onSave, onClose }: WorkflowEditorProps) {
    const [name, setName] = useState(workflow?.name || '');
    const [description, setDescription] = useState(workflow?.description || '');
    const [icon, setIcon] = useState(workflow?.icon || '⚡');
    const [apps, setApps] = useState<string[]>(
        workflow?.apps.map((a) => a.path) || []
    );
    const [urls, setUrls] = useState<string[]>(workflow?.urls || []);
    const [files, setFiles] = useState<string[]>(workflow?.files || []);
    const [newApp, setNewApp] = useState('');
    const [newUrl, setNewUrl] = useState('');
    const [newFile, setNewFile] = useState('');
    const [isSaving, setIsSaving] = useState(false);

    const handleSubmit = async (e: FormEvent) => {
        e.preventDefault();
        if (!name.trim()) return;
        setIsSaving(true);
        try {
            await onSave({
                name: name.trim(),
                description: description.trim(),
                icon,
                apps: apps.map((path) => ({ path, args: [] })),
                urls,
                files,
            });
        } finally {
            setIsSaving(false);
        }
    };

    const addItem = (
        value: string,
        setter: Dispatch<SetStateAction<string>>,
        list: string[],
        listSetter: Dispatch<SetStateAction<string[]>>
    ) => {
        if (value.trim()) {
            listSetter([...list, value.trim()]);
            setter('');
        }
    };

    const removeItem = (
        index: number,
        list: string[],
        listSetter: Dispatch<SetStateAction<string[]>>
    ) => {
        listSetter(list.filter((_, i) => i !== index));
    };

    const emojiOptions = ['⚡', '💻', '🎨', '📁', '🌐', '📧', '🎮', '📊', '🔧', '📝', '🎵', '📸'];

    return (
        <Modal
            isOpen={true}
            onClose={onClose}
            title={workflow ? 'Edit Workflow' : 'Create Workflow'}
            size="lg"
            footer={
                <>
                    <Button variant="secondary" onClick={onClose}>
                        Cancel
                    </Button>
                    <Button variant="primary" onClick={handleSubmit} isLoading={isSaving} disabled={!name.trim()}>
                        {workflow ? 'Save Changes' : 'Create Workflow'}
                    </Button>
                </>
            }
        >
            <form onSubmit={handleSubmit} className="space-y-5">
                {/* Icon */}
                <div>
                    <label className="block text-sm font-medium text-dark-300 mb-2">Icon</label>
                    <div className="flex flex-wrap gap-2">
                        {emojiOptions.map((emoji) => (
                            <button
                                key={emoji}
                                type="button"
                                onClick={() => setIcon(emoji)}
                                className={`w-10 h-10 text-xl rounded-lg flex items-center justify-center transition-colors ${icon === emoji
                                    ? 'bg-primary-600/30 ring-2 ring-primary-500'
                                    : 'bg-dark-800 hover:bg-dark-700'
                                    }`}
                            >
                                {emoji}
                            </button>
                        ))}
                    </div>
                </div>

                {/* Name */}
                <div>
                    <label className="block text-sm font-medium text-dark-300 mb-1">Name</label>
                    <input
                        type="text"
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        placeholder="My Workflow"
                        className="w-full px-3 py-2 bg-dark-800 border border-dark-700 rounded-lg text-white placeholder-dark-400 focus:outline-none focus:ring-2 focus:ring-primary-500"
                    />
                </div>

                {/* Description */}
                <div>
                    <label className="block text-sm font-medium text-dark-300 mb-1">Description</label>
                    <textarea
                        value={description}
                        onChange={(e) => setDescription(e.target.value)}
                        placeholder="What does this workflow do?"
                        rows={2}
                        className="w-full px-3 py-2 bg-dark-800 border border-dark-700 rounded-lg text-white placeholder-dark-400 focus:outline-none focus:ring-2 focus:ring-primary-500 resize-none"
                    />
                </div>

                {/* Apps */}
                <ListEditor
                    label="Applications"
                    icon={<Monitor className="w-4 h-4 text-blue-400" />}
                    items={apps}
                    value={newApp}
                    onChange={setNewApp}
                    onAdd={() => addItem(newApp, setNewApp, apps, setApps)}
                    onRemove={(i) => removeItem(i, apps, setApps)}
                    placeholder="C:\Program Files\app.exe"
                />

                {/* URLs */}
                <ListEditor
                    label="URLs"
                    icon={<Globe className="w-4 h-4 text-green-400" />}
                    items={urls}
                    value={newUrl}
                    onChange={setNewUrl}
                    onAdd={() => addItem(newUrl, setNewUrl, urls, setUrls)}
                    onRemove={(i) => removeItem(i, urls, setUrls)}
                    placeholder="https://example.com"
                />

                {/* Files */}
                <ListEditor
                    label="Files"
                    icon={<FileText className="w-4 h-4 text-amber-400" />}
                    items={files}
                    value={newFile}
                    onChange={setNewFile}
                    onAdd={() => addItem(newFile, setNewFile, files, setFiles)}
                    onRemove={(i) => removeItem(i, files, setFiles)}
                    placeholder="C:\path\to\file.txt"
                />
            </form>
        </Modal>
    );
}

/* ─────────────────────────── List Editor Component ────────────────────────── */

interface ListEditorProps {
    label: string;
    icon: ReactNode;
    items: string[];
    value: string;
    onChange: (v: string) => void;
    onAdd: () => void;
    onRemove: (index: number) => void;
    placeholder: string;
}

function ListEditor({ label, icon, items, value, onChange, onAdd, onRemove, placeholder }: ListEditorProps) {
    return (
        <div>
            <label className="flex items-center gap-2 text-sm font-medium text-dark-300 mb-2">
                {icon}
                {label}
            </label>
            <div className="flex gap-2 mb-2">
                <input
                    type="text"
                    value={value}
                    onChange={(e) => onChange(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && (e.preventDefault(), onAdd())}
                    placeholder={placeholder}
                    className="flex-1 px-3 py-2 bg-dark-800 border border-dark-700 rounded-lg text-white placeholder-dark-400 text-sm focus:outline-none focus:ring-2 focus:ring-primary-500"
                />
                <Button type="button" variant="secondary" size="sm" onClick={onAdd} disabled={!value.trim()}>
                    <Plus className="w-4 h-4" />
                </Button>
            </div>
            {items.length > 0 && (
                <div className="space-y-1">
                    {items.map((item, i) => (
                        <div key={i} className="flex items-center gap-2 px-3 py-1.5 bg-dark-800/50 rounded-lg">
                            <span className="flex-1 text-sm text-dark-300 truncate">{item}</span>
                            <button
                                type="button"
                                onClick={() => onRemove(i)}
                                className="text-dark-500 hover:text-red-400 transition-colors"
                            >
                                <X className="w-4 h-4" />
                            </button>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}
