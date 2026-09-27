import { describe, it, expect } from 'vitest';
import { ApiError } from '@/api/client';
import { pdfExportErrorMessage } from './pdfExport';

describe('pdfExportErrorMessage', () => {
    it('names the first compile error and its line', () => {
        const e = new ApiError(422, 'The note has errors', {
            diagnostics: [
                { severity: 'warning', message: 'unused', line: 1, column: 1, hints: [] },
                { severity: 'error', message: 'unknown variable: x', line: 7, column: 3, hints: [] },
            ],
        });
        expect(pdfExportErrorMessage(e)).toBe("Couldn't export to PDF: unknown variable: x (line 7)");
    });

    it('falls back to the server message, then to any error', () => {
        expect(pdfExportErrorMessage(new ApiError(400, 'Only Typst (.typ) notes can be exported to PDF so far')))
            .toBe("Couldn't export to PDF: Only Typst (.typ) notes can be exported to PDF so far");
        expect(pdfExportErrorMessage(new Error('network down'))).toBe("Couldn't export to PDF: network down");
    });
});
