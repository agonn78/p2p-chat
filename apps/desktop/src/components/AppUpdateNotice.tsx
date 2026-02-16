import { useCallback, useEffect, useMemo, useState } from 'react';
import {
    checkForUpdates,
    downloadAndInstall,
    formatUpdateError,
    onUpdateDownloadEvent,
    restartApp,
    type AvailableUpdate,
    type UpdateDownloadEvent,
} from '../services/updateService';

const CHECK_INTERVAL_MS = 10 * 60 * 1000;

interface DownloadProgress {
    downloaded: number;
    contentLength: number | null;
}

const INITIAL_PROGRESS: DownloadProgress = {
    downloaded: 0,
    contentLength: null,
};

function formatProgress(progress: DownloadProgress): string {
    if (!progress.contentLength || progress.contentLength <= 0) {
        return 'Telechargement...';
    }

    const percentage = Math.min(
        100,
        Math.round((progress.downloaded / progress.contentLength) * 100),
    );
    return `Telechargement ${percentage}%`;
}

function formatPubDate(pubDate?: string | null): string | null {
    if (!pubDate) {
        return null;
    }

    const date = new Date(pubDate);
    if (Number.isNaN(date.getTime())) {
        return null;
    }

    return date.toLocaleString('fr-FR');
}

export function AppUpdateNotice() {
    const [update, setUpdate] = useState<AvailableUpdate | null>(null);
    const [dismissed, setDismissed] = useState(false);
    const [checking, setChecking] = useState(false);
    const [installing, setInstalling] = useState(false);
    const [readyToRestart, setReadyToRestart] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [progress, setProgress] = useState<DownloadProgress>(INITIAL_PROGRESS);

    const onDownloadEvent = useCallback((event: UpdateDownloadEvent) => {
        switch (event.event) {
            case 'Started':
                setProgress({
                    downloaded: 0,
                    contentLength: event.data.contentLength ?? null,
                });
                break;
            case 'Progress':
                setProgress({
                    downloaded: event.data.downloaded,
                    contentLength: event.data.contentLength ?? null,
                });
                break;
            case 'Finished':
                break;
            default:
                break;
        }
    }, []);

    useEffect(() => {
        let cancelled = false;
        let unlisten: Unlisten | null = null;

        onUpdateDownloadEvent(onDownloadEvent)
            .then((fn) => {
                if (cancelled) {
                    void fn();
                    return;
                }
                unlisten = fn;
            })
            .catch((eventError) => {
                console.warn('[Updater] Failed to bind progress listener:', eventError);
            });

        return () => {
            cancelled = true;
            if (unlisten) {
                void unlisten();
            }
        };
    }, [onDownloadEvent]);

    const checkNow = useCallback(async () => {
        if (installing || readyToRestart) {
            return;
        }

        setChecking(true);
        setError(null);

        try {
            const availableUpdate = await checkForUpdates();

            if (!availableUpdate) {
                setUpdate(null);
                setDismissed(false);
                return;
            }

            console.info('[Updater] Update check result:', {
                currentVersion: availableUpdate.currentVersion,
                latestVersion: availableUpdate.latestVersion,
                platform: availableUpdate.platform,
                arch: availableUpdate.arch,
                channel: availableUpdate.channel,
            });

            setUpdate(availableUpdate);
            setDismissed(false);
        } catch (updateError) {
            console.error('[Updater] Update check failed:', updateError);
            setError(formatUpdateError(updateError));
        } finally {
            setChecking(false);
        }
    }, [installing, readyToRestart]);

    useEffect(() => {
        void checkNow();

        const interval = window.setInterval(() => {
            void checkNow();
        }, CHECK_INTERVAL_MS);

        return () => {
            window.clearInterval(interval);
        };
    }, [checkNow]);

    const handleInstall = useCallback(async () => {
        if (!update || installing) {
            return;
        }

        setInstalling(true);
        setError(null);
        setProgress(INITIAL_PROGRESS);

        try {
            await downloadAndInstall();
            setReadyToRestart(true);
            console.info('[Updater] Update install completed');
        } catch (installError) {
            console.error('[Updater] Update install failed:', installError);
            setError(formatUpdateError(installError));
        } finally {
            setInstalling(false);
        }
    }, [installing, update]);

    const handleRestart = useCallback(async () => {
        try {
            await restartApp();
        } catch (restartError) {
            console.error('[Updater] Failed to restart app:', restartError);
            setError(formatUpdateError(restartError));
        }
    }, []);

    const actionLabel = useMemo(() => {
        if (readyToRestart) {
            return 'Redemarrer';
        }
        if (installing) {
            return formatProgress(progress);
        }
        return 'Mettre a jour';
    }, [installing, progress, readyToRestart]);

    if (!update) {
        return null;
    }

    if (dismissed && !update.mandatory && !readyToRestart) {
        return null;
    }

    const publishedAt = formatPubDate(update.pubDate);

    const content = (
        <>
            <div className="space-y-1">
                <p className="text-sm font-semibold text-white">
                    {readyToRestart
                        ? 'Mise a jour installee'
                        : `Nouvelle version ${update.latestVersion} disponible`}
                </p>
                <p className="text-xs text-zinc-300">
                    Version actuelle: {update.currentVersion} - Canal: {update.channel}
                </p>
                {publishedAt && <p className="text-xs text-zinc-400">Publiee: {publishedAt}</p>}
            </div>

            {update.notes && (
                <p className="rounded-md border border-zinc-700 bg-zinc-900/60 p-2 text-xs text-zinc-200 whitespace-pre-wrap">
                    {update.notes}
                </p>
            )}

            {error && (
                <p className="rounded-md border border-red-400/40 bg-red-500/10 p-2 text-xs text-red-200">
                    {error}
                </p>
            )}

            <div className="flex items-center justify-end gap-2">
                {!update.mandatory && !readyToRestart && !installing && (
                    <button
                        type="button"
                        onClick={() => setDismissed(true)}
                        className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200 hover:border-zinc-500"
                    >
                        Plus tard
                    </button>
                )}

                <button
                    type="button"
                    onClick={readyToRestart ? () => void handleRestart() : () => void handleInstall()}
                    disabled={installing || checking}
                    className="rounded-md bg-blue-500 px-3 py-1.5 text-xs font-semibold text-white disabled:cursor-not-allowed disabled:opacity-60"
                >
                    {actionLabel}
                </button>
            </div>

            {update.mandatory && !readyToRestart && (
                <p className="text-xs text-amber-200">
                    Cette mise a jour est obligatoire pour continuer a utiliser l'application.
                </p>
            )}
        </>
    );

    if (update.mandatory && !readyToRestart) {
        return (
            <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/80 p-4">
                <div className="flex w-full max-w-xl flex-col gap-4 rounded-xl border border-zinc-700 bg-zinc-900 p-5 shadow-2xl">
                    {content}
                </div>
            </div>
        );
    }

    return (
        <div className="fixed bottom-4 right-4 z-[120] w-[min(92vw,26rem)] rounded-xl border border-zinc-700 bg-zinc-900/95 p-4 shadow-xl backdrop-blur">
            <div className="flex flex-col gap-3">{content}</div>
        </div>
    );
}

type Unlisten = () => void | Promise<void>;
