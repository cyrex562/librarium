import { computeMergeLayout, type CellLayout } from '@/editor/table-merge-layout';

function escapeHtml(text: string): string {
  return text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

export interface MarkdownFoldRegion {
  type: 'heading' | 'code';
  startLine: number;
  endLine: number;
}

export interface FormattedMarkdownRenderResult {
  html: string;
  foldRegions: MarkdownFoldRegion[];
}

function formatInlineMarkdown(text: string): string {
  let html = escapeHtml(text);

  html = html.replace(/(`+)([^`\n]+?)\1/g, (_match, ticks: string, code: string) => {
    return `<span class="editor-md-inline-code"><span class="editor-md-syntax">${ticks}</span>${code}<span class="editor-md-syntax">${ticks}</span></span>`;
  });

  html = html.replace(/(\[\[[^\]]+\]\])/g, '<span class="editor-md-wikilink">$1</span>');
  html = html.replace(/(\[[^\]]+\]\([^\)]+\))/g, '<span class="editor-md-link">$1</span>');
  html = html.replace(/(^|[\s(])(#[A-Za-z0-9_\/-]+)/g, '$1<span class="editor-md-tag">$2</span>');

  html = html.replace(/(\*\*)(.+?)(\*\*)/g, '<span class="editor-md-strong"><span class="editor-md-syntax">$1</span>$2<span class="editor-md-syntax">$3</span></span>');
  html = html.replace(/(__)(.+?)(__)/g, '<span class="editor-md-strong"><span class="editor-md-syntax">$1</span>$2<span class="editor-md-syntax">$3</span></span>');
  html = html.replace(/(~~)(.+?)(~~)/g, '<span class="editor-md-strikethrough"><span class="editor-md-syntax">$1</span>$2<span class="editor-md-syntax">$3</span></span>');
  html = html.replace(/(==)(.+?)(==)/g, '<span class="editor-md-highlight"><span class="editor-md-syntax">$1</span>$2<span class="editor-md-syntax">$3</span></span>');
  html = html.replace(/(^|[^*])\*(?!\s)([^*]+?)(?<!\s)\*(?!\*)/g, '$1<span class="editor-md-emphasis"><span class="editor-md-syntax">*</span>$2<span class="editor-md-syntax">*</span></span>');
  html = html.replace(/(^|[^_])_(?!\s)([^_]+?)(?<!\s)_(?!_)/g, '$1<span class="editor-md-emphasis"><span class="editor-md-syntax">_</span>$2<span class="editor-md-syntax">_</span></span>');

  return html;
}

function formatMarkdownLine(line: string): string {
  const headingMatch = line.match(/^(\s*)(#{1,6})(\s+)(.*)$/);
  if (headingMatch) {
    const [, indent, hashes, space, rest] = headingMatch;
    return `${escapeHtml(indent)}<span class="editor-md-heading editor-md-heading-${hashes.length}"><span class="editor-md-syntax">${escapeHtml(hashes)}</span>${escapeHtml(space)}${formatInlineMarkdown(rest)}</span>`;
  }

  const blockquoteMatch = line.match(/^(\s*)(>+)(\s?)(.*)$/);
  if (blockquoteMatch) {
    const [, indent, markers, space, rest] = blockquoteMatch;
    return `${escapeHtml(indent)}<span class="editor-md-blockquote"><span class="editor-md-syntax">${escapeHtml(markers)}</span>${escapeHtml(space)}<span class="editor-md-blockquote-content">${formatInlineMarkdown(rest)}</span></span>`;
  }

  const taskMatch = line.match(/^(\s*)((?:[-*+]|\d+[.)]))(\s+)(\[(?: |x|X)\])(\s*)(.*)$/);
  if (taskMatch) {
    const [, indent, marker, gap, checkbox, afterCheckbox, rest] = taskMatch;
    const checked = /\[[xX]\]/.test(checkbox);
    return `${escapeHtml(indent)}<span class="editor-md-list-marker">${escapeHtml(marker)}</span>${escapeHtml(gap)}<span class="editor-md-checkbox ${checked ? 'is-checked' : ''}">${escapeHtml(checkbox)}</span>${escapeHtml(afterCheckbox)}<span class="editor-md-task-text">${formatInlineMarkdown(rest)}</span>`;
  }

  const listMatch = line.match(/^(\s*)((?:[-*+]|\d+[.)]))(\s+)(.*)$/);
  if (listMatch) {
    const [, indent, marker, gap, rest] = listMatch;
    return `${escapeHtml(indent)}<span class="editor-md-list-marker">${escapeHtml(marker)}</span>${escapeHtml(gap)}${formatInlineMarkdown(rest)}`;
  }

  const fencedCodeMatch = line.match(/^(\s*)(```|~~~)(.*)$/);
  if (fencedCodeMatch) {
    const [, indent, fence, rest] = fencedCodeMatch;
    return `${escapeHtml(indent)}<span class="editor-md-fence"><span class="editor-md-syntax">${escapeHtml(fence)}</span>${formatInlineMarkdown(rest)}</span>`;
  }

  const hrMatch = line.match(/^(\s*)([-*_])(?:\s*\2){2,}\s*$/);
  if (hrMatch) {
    return `<span class="editor-md-hr">${escapeHtml(line)}</span>`;
  }

  return formatInlineMarkdown(line);
}

// ── Tables ──────────────────────────────────────────────────────────────────
//
// In formatted mode a GFM table renders as a grid (CSS display: table) rather
// than as pipe-delimited text. The markdown text itself is untouched: pipes,
// the divider row and the row-separating newlines stay in the DOM, just
// hidden with CSS, so textContent is still exactly the source — CodeJar reads
// it back as the document and caret offsets are character offsets into it.

type ColumnAlign = 'left' | 'center' | 'right' | null;

interface TableBlock {
  startLine: number;
  endLine: number;
}

// GFM allows a single hyphen per divider cell (e.g. `:-:`).
const TABLE_DIVIDER_RE = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

/** Indices of the unescaped `|` characters in a line. */
function unescapedPipeIndices(line: string): number[] {
  const indices: number[] = [];
  for (let i = 0; i < line.length; i += 1) {
    if (line[i] === '\\') { i += 1; continue; }
    if (line[i] === '|') indices.push(i);
  }
  return indices;
}

/**
 * Split a table row into alternating pipe/cell segments whose texts,
 * concatenated, reproduce the line exactly. Leading/trailing whitespace next
 * to an edge pipe is folded into that pipe's segment so it hides with it.
 */
function splitTableRow(line: string): Array<{ kind: 'pipe' | 'cell'; text: string }> {
  const pipes = unescapedPipeIndices(line);
  const firstNonWs = line.search(/\S/);
  const lastNonWs = line.length - 1 - (line.match(/\s*$/)?.[0].length ?? 0);
  const hasLead = pipes.length > 0 && pipes[0] === firstNonWs;
  const hasTrail = pipes.length > 0 && pipes[pipes.length - 1] === lastNonWs;

  const segments: Array<{ kind: 'pipe' | 'cell'; text: string }> = [];
  let cursor = 0;
  pipes.forEach((p, idx) => {
    const isLead = idx === 0 && hasLead;
    const isTrail = idx === pipes.length - 1 && hasTrail;
    if (!isLead) segments.push({ kind: 'cell', text: line.slice(cursor, p) });
    const pipeStart = isLead ? 0 : p;
    const pipeEnd = isTrail ? line.length : p + 1;
    segments.push({ kind: 'pipe', text: line.slice(pipeStart, pipeEnd) });
    cursor = pipeEnd;
  });
  if (cursor < line.length) segments.push({ kind: 'cell', text: line.slice(cursor) });
  return segments;
}

function parseColumnAlignments(divider: string): ColumnAlign[] {
  return splitTableRow(divider)
    .filter((s) => s.kind === 'cell')
    .map(({ text }) => {
      const t = text.trim();
      if (t.startsWith(':') && t.endsWith(':')) return 'center';
      if (t.endsWith(':')) return 'right';
      if (t.startsWith(':')) return 'left';
      return null;
    });
}

/**
 * Find GFM tables: a row containing an unescaped pipe, immediately followed by
 * a divider row, then any further non-blank pipe-containing rows. Lines inside
 * fenced code are never tables.
 */
function collectTableBlocks(lines: string[], codeRegions: MarkdownFoldRegion[]): TableBlock[] {
  const codeLines = new Set<number>();
  for (const region of codeRegions) {
    for (let l = region.startLine; l <= region.endLine; l += 1) codeLines.add(l);
  }
  const isRow = (i: number) =>
    i < lines.length && !codeLines.has(i) && lines[i].trim() !== '' && unescapedPipeIndices(lines[i]).length > 0;

  const blocks: TableBlock[] = [];
  for (let i = 0; i < lines.length - 1; i += 1) {
    if (!isRow(i) || !isRow(i + 1) || !TABLE_DIVIDER_RE.test(lines[i + 1])) continue;
    let end = i + 1;
    while (isRow(end + 1) && !TABLE_DIVIDER_RE.test(lines[end + 1])) end += 1;
    blocks.push({ startLine: i, endLine: end });
    i = end;
  }
  return blocks;
}

function cellTexts(line: string): string[] {
  return splitTableRow(line).filter((s) => s.kind === 'cell').map((s) => s.text.trim());
}

/**
 * Merged-cell layout (`^^` / `<<`, see editor/table-merge-layout.ts) for every
 * cell of a table block, keyed `${lineIndex}:${col}`.
 */
function computeCellLayout(lines: string[], block: TableBlock): Map<string, CellLayout> {
  const rowLines: number[] = [];
  for (let line = block.startLine; line <= block.endLine; line += 1) {
    if (line !== block.startLine + 1) rowLines.push(line); // skip the divider row
  }
  const matrix = computeMergeLayout(rowLines.map((line) => cellTexts(lines[line])));
  const layout = new Map<string, CellLayout>();
  rowLines.forEach((line, r) => matrix[r].forEach((cell, c) => layout.set(`${line}:${c}`, cell)));
  return layout;
}

function renderTableRowCells(
  line: string,
  alignments: ColumnAlign[],
  layoutOf: (col: number) => CellLayout | undefined,
): string {
  let col = 0;
  return splitTableRow(line).map((seg) => {
    if (seg.kind === 'pipe') {
      return `<span class="editor-md-table-pipe">${escapeHtml(seg.text)}</span>`;
    }
    const align = alignments[col] ?? null;
    const layout = layoutOf(col);
    col += 1;
    const classes = ['editor-md-table-cell'];
    if (align) classes.push(`is-align-${align}`);
    if (layout?.merged) classes.push('is-merged');
    const spans: string[] = [];
    if (layout && layout.colSpan > 1) spans.push(`grid-column: span ${layout.colSpan}`);
    if (layout && layout.rowSpan > 1) spans.push(`grid-row: span ${layout.rowSpan}`);
    const style = spans.length ? ` style="${spans.join('; ')}"` : '';
    return `<span class="${classes.join(' ')}"${style}>${formatInlineMarkdown(seg.text)}</span>`;
  }).join('');
}

function collectCodeFoldRegions(lines: string[]): MarkdownFoldRegion[] {
  const regions: MarkdownFoldRegion[] = [];

  for (let i = 0; i < lines.length; i += 1) {
    const startMatch = lines[i].match(/^(\s*)(```|~~~)(.*)$/);
    if (!startMatch) continue;

    const fence = startMatch[2];
    let endLine = lines.length - 1;
    for (let j = i + 1; j < lines.length; j += 1) {
      if (new RegExp(`^\\s*${fence}\\s*$`).test(lines[j])) {
        endLine = j;
        break;
      }
    }

    if (endLine > i) {
      regions.push({ type: 'code', startLine: i, endLine });
    }

    i = Math.max(i, endLine);
  }

  return regions;
}

function collectHeadingFoldRegions(lines: string[], codeRegions: MarkdownFoldRegion[]): MarkdownFoldRegion[] {
  const regions: MarkdownFoldRegion[] = [];
  const codeLineSet = new Set<number>();
  for (const region of codeRegions) {
    for (let line = region.startLine; line <= region.endLine; line += 1) {
      codeLineSet.add(line);
    }
  }

  const headings: Array<{ line: number; level: number }> = [];
  for (let i = 0; i < lines.length; i += 1) {
    if (codeLineSet.has(i)) continue;
    const match = lines[i].match(/^\s*(#{1,6})\s+(.+)$/);
    if (match) {
      headings.push({ line: i, level: match[1].length });
    }
  }

  for (let i = 0; i < headings.length; i += 1) {
    const current = headings[i];
    let endLine = lines.length - 1;
    for (let j = i + 1; j < headings.length; j += 1) {
      if (headings[j].level <= current.level) {
        endLine = headings[j].line - 1;
        break;
      }
    }

    if (endLine > current.line) {
      regions.push({ type: 'heading', startLine: current.line, endLine });
    }
  }

  return regions;
}

export function renderFormattedMarkdown(text: string, collapsedStarts: Set<number> = new Set()): FormattedMarkdownRenderResult {
  const lines = text.split('\n');
  const codeRegions = collectCodeFoldRegions(lines);
  const headingRegions = collectHeadingFoldRegions(lines, codeRegions);
  const foldRegions = [...headingRegions, ...codeRegions].sort((a, b) => a.startLine - b.startLine);
  const foldByStart = new Map<number, MarkdownFoldRegion>(foldRegions.map((region) => [region.startLine, region]));
  const headingRegionByStart = new Map<number, MarkdownFoldRegion>(headingRegions.map((region) => [region.startLine, region]));
  const hiddenLines = new Set<number>();
  const headingDepthByLine = new Array(lines.length).fill(0);

  const activeHeadingEnds: number[] = [];
  for (let i = 0; i < lines.length; i += 1) {
    while (activeHeadingEnds.length > 0 && i > activeHeadingEnds[activeHeadingEnds.length - 1]) {
      activeHeadingEnds.pop();
    }
    headingDepthByLine[i] = activeHeadingEnds.length;

    const headingRegion = headingRegionByStart.get(i);
    if (headingRegion) {
      activeHeadingEnds.push(headingRegion.endLine);
    }
  }

  for (const startLine of collapsedStarts) {
    const region = foldByStart.get(startLine);
    if (!region) continue;
    for (let line = region.startLine + 1; line <= region.endLine; line += 1) {
      hiddenLines.add(line);
    }
  }

  const tableBlocks = collectTableBlocks(lines, codeRegions);
  const tableByStart = new Map(tableBlocks.map((block) => [block.startLine, block]));

  const renderLine = (line: string, index: number) => {
    const region = foldByStart.get(index);
    const isCollapsed = region ? collapsedStarts.has(index) : false;
    const hiddenClass = hiddenLines.has(index) ? ' is-hidden' : '';
    const depth = headingDepthByLine[index] ?? 0;
    const nestedClass = depth > 0 ? ' is-nested' : '';
    const hiddenLineCount = region ? (region.endLine - region.startLine) : 0;
    const toggle = region
      ? `<span contenteditable="false" class="editor-md-fold-toggle${isCollapsed ? ' is-collapsed' : ''}" data-fold-start="${index}" title="${isCollapsed ? 'Expand' : 'Collapse'} ${region.type === 'heading' ? 'section' : 'code block'}"></span>`
      : '<span contenteditable="false" class="editor-md-fold-spacer"></span>';
    const summary = region && isCollapsed
      ? `<span contenteditable="false" class="editor-md-fold-summary" data-hidden-lines="${hiddenLineCount}"></span>`
      : '';

    return `<span class="editor-md-line${hiddenClass}${nestedClass}" style="--fold-depth:${depth}" data-line="${index}">${toggle}${formatMarkdownLine(line)}${summary}</span>`;
  };

  // A table renders as one grid block. Its rows are display: table-row, so a
  // bare "\n" text node between them would become an anonymous table cell;
  // instead each row's newline lives in a hidden span at the end of the row.
  // Headings and code fences are never table rows, so no fold toggle or
  // summary can occur inside a block.
  const renderTable = (block: TableBlock) => {
    const alignments = parseColumnAlignments(lines[block.startLine + 1]);
    const layout = computeCellLayout(lines, block);
    const colCount = cellTexts(lines[block.startLine]).length;
    const depth = headingDepthByLine[block.startLine] ?? 0;
    const nestedClass = depth > 0 ? ' is-nested' : '';
    let allHidden = true;
    const rows: string[] = [];
    for (let index = block.startLine; index <= block.endLine; index += 1) {
      const line = lines[index];
      const hidden = hiddenLines.has(index);
      if (!hidden) allHidden = false;
      const roleClass = index === block.startLine ? ' is-header' : index === block.startLine + 1 ? ' is-divider' : '';
      const body = index === block.startLine + 1
        ? escapeHtml(line)
        : renderTableRowCells(line, alignments, (col) => layout.get(`${index}:${col}`));
      const eol = index < block.endLine ? '<span class="editor-md-table-eol">\n</span>' : '';
      rows.push(`<span class="editor-md-line editor-md-table-row${roleClass}${hidden ? ' is-hidden' : ''}" data-line="${index}">${body}${eol}</span>`);
    }
    return `<span class="editor-md-table${nestedClass}${allHidden ? ' is-hidden' : ''}" style="--fold-depth:${depth}; --table-cols:${colCount}">${rows.join('')}</span>`;
  };

  const pieces: string[] = [];
  for (let index = 0; index < lines.length; index += 1) {
    const block = tableByStart.get(index);
    if (block) {
      pieces.push(renderTable(block));
      index = block.endLine;
    } else {
      pieces.push(renderLine(lines[index], index));
    }
  }
  const html = pieces.join('\n');

  return { html, foldRegions };
}

/** Called by CodeJar to keep the editor in plain text mode without syntax styling. */
export function highlightPlainText(editor: HTMLElement) {
  const text = editor.textContent ?? '';
  if (editor.childNodes.length !== 1 || editor.firstChild?.nodeType !== Node.TEXT_NODE || editor.textContent !== text) {
    editor.textContent = text;
  }
}
