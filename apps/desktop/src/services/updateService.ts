import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface AvailableUpdate {
    currentVersion: string;
    latestVersion: string;
    mandatory: boolean;
    notes?: string | null;
    pubDate?: string | null;
    channel: string;
    platform: string;
    arch: string;
    url: string;
    sha256?: string | null;
}

export type UpdateDownloadEvent =
    | { event: 'Started'; data: { contentLength?: number | null } }
    | {
        event: 'Progress';
        data: { chunkLength: number; downloaded: number; contentLength?: number | null };
    }
    | { event: 'Finished' };

const UPDATE_DOWNLOAD_EVENT = 'app-update-download';

export async function checkForUpdates(): Promise<AvailableUpdate | null> {
    return invoke<AvailableUpdate | null>('app_check_for_updates');
}

export async function downloadAndInstall(): Promise<void> {
    await invoke('app_download_and_install_update');
}

export async function restartApp(): Promise<void> {
    await invoke('app_restart_after_update');
}

export async function onUpdateDownloadEvent(
    handler: (event: UpdateDownloadEvent) => void,
): Promise<UnlistenFn> {
    return listen<UpdateDownloadEvent>(UPDATE_DOWNLOAD_EVENT, (event) => {
        handler(event.payload);
    });
}

export function formatUpdateError(error: unknown): string {
    if (typeof error === 'string') {
        return error;
    }

    if (error && typeof error === 'object') {
        const maybe = error as {
            message?: string;
            details?: string;
            code?: string;
        };

        if (maybe.details) {
            return maybe.details;
        }
        if (maybe.message) {
            return maybe.message;
        }
        if (maybe.code) {
            return `Update error (${maybe.code})`;
        }
    }

    return 'Une erreur inconnue est survenue pendant la mise a jour.';
}
