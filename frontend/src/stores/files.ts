import { defineStore } from 'pinia';
import { ref } from 'vue';
import {
    apiGetFileTree,
    apiReadFile,
    apiWriteFile,
    apiCreateFile,
    apiDeleteFile,
    apiCreateDirectory,
    apiRenameFile,
    apiGetRandomNote,
    apiGetDailyNote,
    apiGetRecentFiles,
    apiRecordRecentFile,
    apiCreateUploadSession,
    apiUploadChunk,
    apiFinishUploadSession,
    apiImportArchive,
    apiDownloadZip,
    apiDownloadTar,
    ApiError,
} from '@/api/client';
import type {
    FileNode,
    FileContent,
    UpdateFileRequest,
    ImportCandidate,
    ImportProgress,
    ImportResult,
    ImportResultItem,
} from '@/api/types';

function normalizePath(value: string): string {
    return value
        .replace(/\\/g, '/')
        .split('/')
        .filter(Boolean)
        .join('/');
}

function joinPath(...segments: Array<string | undefined>): string {
    return normalizePath(segments.filter(Boolean).join('/'));
}

function dirname(filePath: string): string {
    const normalized = normalizePath(filePath);
    const idx = normalized.lastIndexOf('/');
    return idx >= 0 ? normalized.slice(0, idx) : '';
}

function basename(filePath: string): string {
    const normalized = normalizePath(filePath);
    const idx = normalized.lastIndexOf('/');
    return idx >= 0 ? normalized.slice(idx + 1) : normalized;
}

function walkNodes(nodes: FileNode[], visit: (node: FileNode) => void): void {
    for (const node of nodes) {
        visit(node);
        if (node.children) walkNodes(node.children, visit);
    }
}

function isArchiveFile(filename: string): boolean {
    const lower = filename.toLowerCase();
    return lower.endsWith('.zip') || lower.endsWith('.tar') || lower.endsWith('.tar.gz') || lower.endsWith('.tgz');
}

function triggerBlobDownload(blob: Blob, filename: string): void {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
}

export const useFilesStore = defineStore('files', () => {
    const tree = ref<FileNode[]>([]);
    const recentFiles = ref<string[]>([]);
    const loading = ref(false);
    const error = ref<string | null>(null);
    const selectionMode = ref(false);
    const selectedPaths = ref<Set<string>>(new Set());
    const lastSelectionAnchorPath = ref<string | null>(null);
    const collapseAllFoldersVersion = ref(0);

    async function loadTree(vaultId: string) {
        loading.value = true;
        error.value = null;
        try {
            tree.value = await apiGetFileTree(vaultId);
        } catch (e) {
            error.value = String(e);
        } finally {
            loading.value = false;
        }
    }

    async function readFile(vaultId: string, filePath: string): Promise<FileContent> {
        return apiReadFile(vaultId, filePath);
    }

    async function writeFile(
        vaultId: string,
        filePath: string,
        data: UpdateFileRequest,
    ): Promise<FileContent> {
        return apiWriteFile(vaultId, filePath, data);
    }

    async function createFile(
        vaultId: string,
        filePath: string,
        content = '',
    ): Promise<FileContent> {
        const result = await apiCreateFile(vaultId, { path: filePath, content });
        // Refresh tree after mutation
        await loadTree(vaultId);
        return result;
    }

    async function deleteFile(vaultId: string, filePath: string) {
        await apiDeleteFile(vaultId, filePath);
        await loadTree(vaultId);
    }

    async function deleteFiles(vaultId: string, paths: string[]) {
        for (const path of paths) {
            await apiDeleteFile(vaultId, path);
        }
        clearSelection();
        await loadTree(vaultId);
    }

    async function createDirectory(vaultId: string, path: string) {
        await apiCreateDirectory(vaultId, path);
        await loadTree(vaultId);
    }

    async function renameFile(
        vaultId: string,
        from: string,
        to: string,
        strategy: 'fail' | 'overwrite' | 'rename' = 'fail',
    ): Promise<string> {
        const result = await apiRenameFile(vaultId, from, to, strategy);
        await loadTree(vaultId);
        return result.new_path;
    }

    async function moveFiles(
        vaultId: string,
        moves: Array<{ from: string; to: string }>,
        strategy: 'fail' | 'overwrite' | 'rename' = 'fail',
    ): Promise<Array<{ from: string; to: string }>> {
        const completed: Array<{ from: string; to: string }> = [];
        try {
            for (const move of moves) {
                const result = await apiRenameFile(vaultId, move.from, move.to, strategy);
                completed.push({ from: move.from, to: result.new_path });
            }
            return completed;
        } finally {
            await loadTree(vaultId);
            clearSelection();
        }
    }

    async function getRandomNote(vaultId: string): Promise<string> {
        const result = await apiGetRandomNote(vaultId);
        return result.path;
    }

    async function getDailyNote(vaultId: string): Promise<FileContent> {
        const today = new Date().toISOString().split('T')[0];
        return apiGetDailyNote(vaultId, today);
    }

    async function loadRecentFiles(vaultId: string) {
        try {
            recentFiles.value = await apiGetRecentFiles(vaultId);
        } catch {
            recentFiles.value = [];
        }
    }

    function recordRecentFile(vaultId: string, filePath: string) {
        // Optimistic local update
        recentFiles.value = [
            filePath,
            ...recentFiles.value.filter((p) => p !== filePath),
        ].slice(0, 20);
        apiRecordRecentFile(vaultId, filePath);
    }

    async function createDirectoryIfMissing(vaultId: string, path: string) {
        if (!path) return;

        try {
            await apiCreateDirectory(vaultId, path);
        } catch (error) {
            if (error instanceof ApiError && error.status === 409) {
                return;
            }
            throw error;
        }
    }

    async function uploadCandidateFile(
        vaultId: string,
        candidate: ImportCandidate,
        targetDirectory: string,
        onProgress?: (uploadedBytes: number) => void,
        conflict: 'fail' | 'overwrite' | 'skip' | 'rename_with_timestamp' = 'rename_with_timestamp',
        signal?: AbortSignal,
    ): Promise<ImportResultItem> {
        throwIfAborted(signal);
        const session = await apiCreateUploadSession(
            vaultId,
            candidate.file.name,
            candidate.file.size,
            targetDirectory,
        );

        const chunkSize = 2 * 1024 * 1024;
        let uploadedBytes = 0;

        while (uploadedBytes < candidate.file.size) {
            throwIfAborted(signal);
            const end = Math.min(uploadedBytes + chunkSize, candidate.file.size);
            const chunk = candidate.file.slice(uploadedBytes, end);
            const response = await apiUploadChunk(vaultId, session.session_id, chunk);
            uploadedBytes = response.uploaded_bytes;
            onProgress?.(uploadedBytes);
        }

        throwIfAborted(signal);
        return apiFinishUploadSession(
            vaultId,
            session.session_id,
            candidate.file.name,
            targetDirectory,
            conflict,
        );
    }

    function throwIfAborted(signal?: AbortSignal) {
        if (signal?.aborted) {
            throw new DOMException('Import canceled.', 'AbortError');
        }
    }

    async function importCandidates(
        vaultId: string,
        candidates: ImportCandidate[],
        targetPath = '',
        onProgress?: (progress: ImportProgress) => void,
        conflict: 'fail' | 'overwrite' | 'skip' | 'rename_with_timestamp' = 'rename_with_timestamp',
        signal?: AbortSignal,
    ): Promise<ImportResult> {
        const normalizedTarget = normalizePath(targetPath);

        // Separate regular files from archives (.zip / .tar / .tar.gz / .tgz)
        const archiveCandidates = candidates.filter((c) => isArchiveFile(c.file.name));
        const regularCandidates = candidates.filter((c) => !isArchiveFile(c.file.name));

        const totalFiles = candidates.length;
        const totalBytes = candidates.reduce((sum, candidate) => sum + candidate.file.size, 0);
        const uploaded: ImportResultItem[] = [];
        const skipped: ImportResultItem[] = [];
        let completedFiles = 0;
        let baseUploadedBytes = 0;

        onProgress?.({
            totalFiles,
            completedFiles,
            totalBytes,
            uploadedBytes: 0,
        });

        try {
            // ── 1. Extract archives via the dedicated endpoint ───────────────────
            for (const candidate of archiveCandidates) {
                throwIfAborted(signal);
                const currentFile = candidate.relativePath;
                onProgress?.({
                    totalFiles,
                    completedFiles,
                    totalBytes,
                    uploadedBytes: baseUploadedBytes,
                    currentFile,
                });

                const result = await apiImportArchive(vaultId, candidate.file, normalizedTarget, conflict);
                // Represent each extracted path as a pseudo-result item
                for (const extractedPath of result.extracted) {
                    uploaded.push({ path: extractedPath, filename: extractedPath.split('/').pop() ?? '', size: 0 });
                }
                for (const skippedPath of result.skipped ?? []) {
                    skipped.push({ path: skippedPath, filename: skippedPath.split('/').pop() ?? '', size: 0, skipped: true });
                }
                completedFiles += 1;
                baseUploadedBytes += candidate.file.size;
                onProgress?.({
                    totalFiles,
                    completedFiles,
                    totalBytes,
                    uploadedBytes: baseUploadedBytes,
                });
            }

            // ── 2. Pre-create directories for regular files ──────────────────────
            const directories = new Set<string>();
            for (const candidate of regularCandidates) {
                const relativeDir = dirname(candidate.relativePath);
                const destinationDir = joinPath(normalizedTarget, relativeDir);
                if (destinationDir) {
                    const segments = destinationDir.split('/');
                    for (let i = 0; i < segments.length; i += 1) {
                        directories.add(segments.slice(0, i + 1).join('/'));
                    }
                }
            }

            const orderedDirectories = [...directories].sort((a, b) => a.split('/').length - b.split('/').length);
            for (const directory of orderedDirectories) {
                throwIfAborted(signal);
                await createDirectoryIfMissing(vaultId, directory);
            }

            // ── 3. Upload regular files ──────────────────────────────────────────
            for (const candidate of regularCandidates) {
                throwIfAborted(signal);
                const relativeDir = dirname(candidate.relativePath);
                const destinationDir = joinPath(normalizedTarget, relativeDir);
                const currentFile = candidate.relativePath;

                const result = await uploadCandidateFile(vaultId, candidate, destinationDir, (fileUploadedBytes) => {
                    onProgress?.({
                        totalFiles,
                        completedFiles,
                        totalBytes,
                        uploadedBytes: baseUploadedBytes + fileUploadedBytes,
                        currentFile,
                    });
                }, conflict, signal);

                if (result.skipped) {
                    skipped.push({ ...result, size: candidate.file.size });
                } else {
                    uploaded.push(result);
                }
                completedFiles += 1;
                baseUploadedBytes += candidate.file.size;
                onProgress?.({
                    totalFiles,
                    completedFiles,
                    totalBytes,
                    uploadedBytes: baseUploadedBytes,
                    currentFile,
                });
            }

            return {
                uploaded,
                skipped,
                directoryCount: orderedDirectories.length,
                totalBytes,
            };
        } finally {
            await loadTree(vaultId);
        }
    }

    /** Download selected vault paths as a ZIP file and trigger a browser download. */
    async function downloadAsZip(vaultId: string, paths: string[]): Promise<void> {
        const blob = await apiDownloadZip(vaultId, paths);
        triggerBlobDownload(blob, paths.length === 1 ? `${paths[0].split('/').pop() ?? 'download'}.zip` : `${paths.length}_files.zip`);
    }

    /** Download selected vault paths as a tar.gz and trigger a browser download. */
    async function downloadAsTar(vaultId: string, paths: string[]): Promise<void> {
        const blob = await apiDownloadTar(vaultId, paths);
        triggerBlobDownload(blob, paths.length === 1 ? `${paths[0].split('/').pop() ?? 'download'}.tar.gz` : `${paths.length}_files.tar.gz`);
    }

    function setSelectionMode(enabled: boolean) {
        selectionMode.value = enabled;
        if (!enabled) clearSelection();
    }

    function toggleSelectionMode() {
        setSelectionMode(!selectionMode.value);
    }

    function selectPath(path: string) {
        selectedPaths.value = new Set([...selectedPaths.value, path]);
        lastSelectionAnchorPath.value = path;
    }

    function deselectPath(path: string) {
        const next = new Set(selectedPaths.value);
        next.delete(path);
        selectedPaths.value = next;
        lastSelectionAnchorPath.value = path;
    }

    function toggleSelectedPath(path: string) {
        if (selectedPaths.value.has(path)) {
            deselectPath(path);
            return;
        }
        selectPath(path);
    }

    function clearSelection() {
        selectedPaths.value = new Set();
        lastSelectionAnchorPath.value = null;
    }

    function isSelected(path: string) {
        return selectedPaths.value.has(path);
    }

    function selectedNodes(): FileNode[] {
        const selected: FileNode[] = [];
        walkNodes(tree.value, (node) => {
            if (selectedPaths.value.has(node.path)) selected.push(node);
        });
        return selected;
    }

    function selectedTopLevelNodes(): FileNode[] {
        return selectedNodes().filter((node) => {
            for (const path of selectedPaths.value) {
                if (path !== node.path && node.path.startsWith(`${path}/`)) {
                    return false;
                }
            }
            return true;
        });
    }

    function selectPathRange(orderedPaths: string[], targetPath: string) {
        const anchorPath = lastSelectionAnchorPath.value;
        if (!anchorPath) {
            selectPath(targetPath);
            return;
        }

        const anchorIndex = orderedPaths.indexOf(anchorPath);
        const targetIndex = orderedPaths.indexOf(targetPath);
        if (anchorIndex < 0 || targetIndex < 0) {
            selectPath(targetPath);
            return;
        }

        const start = Math.min(anchorIndex, targetIndex);
        const end = Math.max(anchorIndex, targetIndex);
        selectedPaths.value = new Set([
            ...selectedPaths.value,
            ...orderedPaths.slice(start, end + 1),
        ]);
    }

    function handleSelectionClick(
        path: string,
        orderedPaths: string[],
        options: { range?: boolean; toggle?: boolean } = {},
    ) {
        selectionMode.value = true;
        if (options.range) {
            selectPathRange(orderedPaths, path);
            return;
        }

        if (options.toggle || selectionMode.value) {
            toggleSelectedPath(path);
            return;
        }

        selectedPaths.value = new Set([path]);
        lastSelectionAnchorPath.value = path;
    }

    function destinationExists(path: string) {
        let exists = false;
        walkNodes(tree.value, (node) => {
            if (node.path === path) exists = true;
        });
        return exists;
    }

    function buildMoveTargets(paths: string[], destinationDirectory: string) {
        return paths.map((from) => ({
            from,
            to: normalizePath(`${destinationDirectory}/${basename(from)}`),
        }));
    }

    function collapseAllFolders() {
        collapseAllFoldersVersion.value += 1;
    }

    return {
        tree,
        recentFiles,
        loading,
        error,
        loadTree,
        readFile,
        writeFile,
        createFile,
        deleteFile,
        deleteFiles,
        createDirectory,
        renameFile,
        moveFiles,
        getRandomNote,
        getDailyNote,
        loadRecentFiles,
        recordRecentFile,
        importCandidates,
        downloadAsZip,
        downloadAsTar,
        selectionMode,
        selectedPaths,
        lastSelectionAnchorPath,
        collapseAllFoldersVersion,
        setSelectionMode,
        toggleSelectionMode,
        selectPath,
        deselectPath,
        toggleSelectedPath,
        clearSelection,
        selectPathRange,
        handleSelectionClick,
        isSelected,
        selectedNodes,
        selectedTopLevelNodes,
        destinationExists,
        buildMoveTargets,
        collapseAllFolders,
    };
});
