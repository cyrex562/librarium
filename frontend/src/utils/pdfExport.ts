import { ApiError, type TypstDiagnostic } from '@/api/client';

/** One-line explanation of a failed PDF export (#140), for an alert. */
export function pdfExportErrorMessage(e: unknown): string {
    if (e instanceof ApiError) {
        const body = e.body as { diagnostics?: TypstDiagnostic[] } | undefined;
        const first = body?.diagnostics?.find((d) => d.severity === 'error');
        if (first) {
            const where = first.line ? ` (line ${first.line})` : '';
            return `Couldn't export to PDF: ${first.message}${where}`;
        }
        return `Couldn't export to PDF: ${e.message}`;
    }
    return `Couldn't export to PDF: ${e instanceof Error ? e.message : String(e)}`;
}
