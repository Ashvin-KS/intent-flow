import { useState, useEffect, useCallback } from 'react';
import { getRecentModels, removeRecentModel } from '../services/tauri';

export interface FavoriteModel {
    id: string;
    name: string;
    last_used?: number;
    use_count?: number;
}

const STORAGE_KEY = 'intentflow_recent_models';

function loadFromStorage(): FavoriteModel[] {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (raw) return JSON.parse(raw);
    } catch { }
    return [];
}

function saveToStorage(models: FavoriteModel[]) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(models));
}

export function useFavoriteModels() {
    const [favorites, setFavorites] = useState<FavoriteModel[]>(loadFromStorage);

    const refreshRecent = useCallback(() => {
        getRecentModels(5)
            .then((models) => {
                setFavorites(models.map((m) => ({
                    id: m.id,
                    name: m.name,
                    last_used: m.last_used,
                    use_count: m.use_count,
                })));
            })
            .catch(() => {
                // Fallback to local storage only.
            });
    }, []);

    useEffect(() => {
        saveToStorage(favorites);
    }, [favorites]);

    useEffect(() => {
        refreshRecent();
        const timer = window.setInterval(refreshRecent, 5000);
        return () => {
            window.clearInterval(timer);
        };
    }, [refreshRecent]);

    const addFavorite = useCallback((model: FavoriteModel) => {
        setFavorites((prev) => {
            const existing = prev.filter((m) => m.id !== model.id);
            return [{ ...model, last_used: Date.now() / 1000 }, ...existing].slice(0, 5);
        });
    }, []);

    const removeFavorite = useCallback((modelId: string) => {
        setFavorites((prev) => prev.filter((m) => m.id !== modelId));
        removeRecentModel(modelId)
            .then(() => refreshRecent())
            .catch(() => {
                // keep local removal even if DB call fails
            });
    }, [refreshRecent]);

    const isFavorite = useCallback(
        (modelId: string) => favorites.some((m) => m.id === modelId),
        [favorites]
    );

    return { favorites, addFavorite, removeFavorite, isFavorite, setFavorites, refreshRecent };
}
