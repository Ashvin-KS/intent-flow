import { useState, useEffect, useCallback } from 'react';
import { getWorkflows, createWorkflow, updateWorkflow, deleteWorkflow, executeWorkflow, getWorkflowSuggestions } from '../services/tauri';
import type { Workflow, WorkflowSuggestion } from '../types';

export function useWorkflows() {
    const [workflows, setWorkflows] = useState<Workflow[]>([]);
    const [suggestions, setSuggestions] = useState<WorkflowSuggestion[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const [workflowsData, suggestionsData] = await Promise.all([
                getWorkflows(),
                getWorkflowSuggestions(),
            ]);
            setWorkflows(workflowsData);
            setSuggestions(suggestionsData);
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to load workflows');
        } finally {
            setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        refresh();
    }, [refresh]);

    const create = useCallback(async (data: Omit<Workflow, 'id' | 'use_count' | 'last_used' | 'created_at'>) => {
        try {
            await createWorkflow(data);
            await refresh();
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to create workflow');
            throw err;
        }
    }, [refresh]);

    const update = useCallback(async (workflow: Workflow) => {
        try {
            await updateWorkflow(workflow);
            await refresh();
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to update workflow');
            throw err;
        }
    }, [refresh]);

    const remove = useCallback(async (id: string) => {
        try {
            await deleteWorkflow(id);
            await refresh();
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to delete workflow');
            throw err;
        }
    }, [refresh]);

    const execute = useCallback(async (id: string) => {
        try {
            await executeWorkflow(id);
            await refresh();
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to execute workflow');
            throw err;
        }
    }, [refresh]);

    return { workflows, suggestions, isLoading, error, refresh, create, update, remove, execute };
}
