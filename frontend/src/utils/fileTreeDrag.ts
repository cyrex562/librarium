export const FILE_TREE_DRAG_TYPE = 'application/x-obsidian-host-tree-node';

export interface FileTreeDragItem {
    path: string;
    name: string;
    isDirectory: boolean;
}

export interface FileTreeDragPayload extends FileTreeDragItem {
    items?: FileTreeDragItem[];
}

export function setFileTreeDragPayload(dataTransfer: DataTransfer, payload: FileTreeDragPayload) {
    const serialized = JSON.stringify(payload);
    dataTransfer.setData(FILE_TREE_DRAG_TYPE, serialized);
    // Fallback so some browsers keep the drag operation alive.
    dataTransfer.setData('text/plain', getFileTreeDragItems(payload).map((item) => item.path).join('\n'));
    dataTransfer.effectAllowed = 'move';
}

export function getFileTreeDragPayload(dataTransfer?: DataTransfer | null): FileTreeDragPayload | null {
    if (!dataTransfer || !Array.from(dataTransfer.types).includes(FILE_TREE_DRAG_TYPE)) {
        return null;
    }

    try {
        return JSON.parse(dataTransfer.getData(FILE_TREE_DRAG_TYPE)) as FileTreeDragPayload;
    } catch {
        return null;
    }
}

export function getFileTreeDragItems(payload: FileTreeDragPayload): FileTreeDragItem[] {
    return payload.items && payload.items.length > 0
        ? payload.items
        : [{ path: payload.path, name: payload.name, isDirectory: payload.isDirectory }];
}

export function hasFileTreeDragPayload(dataTransfer?: DataTransfer | null): boolean {
    return getFileTreeDragPayload(dataTransfer) !== null;
}
