import { useEffect, useRef } from 'react';
import { X } from 'lucide-react';
import { SettingsPanel } from './Settings';

interface SettingsModalProps {
    isOpen: boolean;
    onClose: () => void;
}

export function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
    const panelRef = useRef<HTMLDivElement>(null);

    // Close on Escape key
    useEffect(() => {
        if (!isOpen) return;
        const handler = (e: KeyboardEvent) => {
            if (e.key === 'Escape') onClose();
        };
        window.addEventListener('keydown', handler);
        return () => window.removeEventListener('keydown', handler);
    }, [isOpen, onClose]);

    // Prevent body scroll when open
    useEffect(() => {
        if (isOpen) {
            document.body.style.overflow = 'hidden';
        } else {
            document.body.style.overflow = '';
        }
        return () => { document.body.style.overflow = ''; };
    }, [isOpen]);

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-[100] flex items-center justify-center">
            {/* Backdrop */}
            <div
                className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                onClick={onClose}
            />

            {/* Modal Panel */}
            <div
                ref={panelRef}
                className="relative z-10 w-[90vw] max-w-4xl h-[80vh] max-h-[700px] bg-dark-900 border border-dark-700/50 rounded-2xl shadow-2xl shadow-black/50 flex flex-col overflow-hidden animate-modal-in"
            >
                {/* Modal Header */}
                <div className="flex items-center justify-between px-6 py-4 border-b border-dark-800">
                    <h2 className="text-lg font-semibold text-white">Settings</h2>
                    <button
                        onClick={onClose}
                        className="w-8 h-8 flex items-center justify-center rounded-lg text-dark-400 hover:text-white hover:bg-dark-800 transition-colors"
                        title="Close settings"
                    >
                        <X className="w-4 h-4" />
                    </button>
                </div>

                {/* Settings Content (scrollable) */}
                <div className="flex-1 overflow-y-auto p-6">
                    <SettingsPanel />
                </div>
            </div>
        </div>
    );
}
