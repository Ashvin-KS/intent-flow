import { useState, useEffect, useCallback } from 'react';
import { getSettings, updateSettings as updateSettingsApi, getCategories, updateCategories as updateCategoriesApi } from '../services/tauri';
import type { Settings, Category } from '../types';

export function useSettings() {
    const [settings, setSettings] = useState<Settings | null>(null);
    const [categories, setCategories] = useState<Category[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const [settingsData, categoriesData] = await Promise.all([
                getSettings(),
                getCategories(),
            ]);
            setSettings(settingsData);
            setCategories(categoriesData);
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to load settings');
        } finally {
            setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        refresh();
    }, [refresh]);

    const updateSettings = useCallback(async (newSettings: Settings) => {
        setIsSaving(true);
        setError(null);
        try {
            await updateSettingsApi(newSettings);
            setSettings(newSettings);
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to save settings');
            throw err;
        } finally {
            setIsSaving(false);
        }
    }, []);

    const updateCategories = useCallback(async (newCategories: Category[]) => {
        setIsSaving(true);
        setError(null);
        try {
            await updateCategoriesApi(newCategories);
            setCategories(newCategories);
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to save categories');
            throw err;
        } finally {
            setIsSaving(false);
        }
    }, []);

    return { settings, categories, isLoading, isSaving, error, refresh, updateSettings, updateCategories };
}
