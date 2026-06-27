import { vi } from 'vitest';

// happy-dom's `localStorage` can be a non-functional stub in this setup (it
// emits a `--localstorage-file` warning and `clear`/`removeItem` are not
// callable), which breaks any test that touches storage. Install a complete
// in-memory Storage implementation so tests have predictable, working storage.
class MemoryStorage implements Storage {
    private store = new Map<string, string>();

    get length(): number {
        return this.store.size;
    }

    clear(): void {
        this.store.clear();
    }

    getItem(key: string): string | null {
        return this.store.has(key) ? (this.store.get(key) as string) : null;
    }

    key(index: number): string | null {
        return [...this.store.keys()][index] ?? null;
    }

    removeItem(key: string): void {
        this.store.delete(key);
    }

    setItem(key: string, value: string): void {
        this.store.set(key, String(value));
    }
}

vi.stubGlobal('localStorage', new MemoryStorage());
