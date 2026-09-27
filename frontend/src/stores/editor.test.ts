import { describe, it, expect, beforeEach, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { useEditorStore } from './editor';

describe('editor store auto-save', () => {
    beforeEach(() => {
        setActivePinia(createPinia());
        vi.useFakeTimers();
    });

    it('flushAutoSave runs the pending save now, once', async () => {
        const store = useEditorStore();
        const save = vi.fn(async () => {});
        store.scheduleAutoSave('tab-1', 2000, save);

        await store.flushAutoSave('tab-1');
        expect(save).toHaveBeenCalledTimes(1);

        vi.advanceTimersByTime(5000);
        expect(save).toHaveBeenCalledTimes(1);
    });

    it('flushAutoSave with nothing pending does nothing', async () => {
        const store = useEditorStore();
        const save = vi.fn();
        store.scheduleAutoSave('tab-1', 2000, save);
        vi.advanceTimersByTime(2000);
        expect(save).toHaveBeenCalledTimes(1);

        await store.flushAutoSave('tab-1');
        await store.flushAutoSave('other-tab');
        expect(save).toHaveBeenCalledTimes(1);
    });

    it('cancelAutoSave also drops the pending save', async () => {
        const store = useEditorStore();
        const save = vi.fn();
        store.scheduleAutoSave('tab-1', 2000, save);
        store.cancelAutoSave('tab-1');
        await store.flushAutoSave('tab-1');
        vi.advanceTimersByTime(5000);
        expect(save).not.toHaveBeenCalled();
    });
});
