// Adapted from rpg_creator scripts/audit_editor_design_system.mjs at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import fs from 'node:fs';
import path from 'node:path';
import * as sass from 'sass';

const root = path.resolve(import.meta.dirname, '..');
const sourceRoot = path.join(root, 'src');
const violations = [];
const counts = new Map();
const semanticClasses = new Set();
const auditedTags = new Set(['button', 'input', 'select', 'textarea', 'label', 'fieldset', 'legend', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'p', 'small', 'code', 'pre']);
function filesBelow(entry) {
  return fs.readdirSync(entry, {withFileTypes: true}).flatMap(item => {
    const file = path.join(entry, item.name);
    if (item.isDirectory()) return item.name === 'pkg' ? [] : filesBelow(file);
    return [file];
  });
}
function openingEnd(source, start) {
  let braceDepth = 0;
  let quote = null;
  let escaped = false;
  for (let index = start; index < source.length; index += 1) {
    const char = source[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === quote) quote = null;
      continue;
    }
    if (char === '"' || char === "'" || char === '`') {
      quote = char;
      continue;
    }
    if (char === '{') braceDepth += 1;
    else if (char === '}') braceDepth -= 1;
    else if (char === '>' && braceDepth === 0) return index;
  }
  return -1;
}

function requiredClasses(tag) {
  if (tag === 'button') return ['editor-button'];
  if (tag === 'input') return ['editor-control', 'editor-control--'];
  if (tag === 'select') {
    return ['editor-control', 'editor-control--select'];
  }
  if (tag === 'textarea') {
    return ['editor-control', 'editor-control--textarea'];
  }
  if (tag === 'label') return ['editor-field-label'];
  if (tag === 'fieldset') return ['editor-field-group'];
  if (tag === 'legend') return ['editor-field-legend'];
  if (/^h[1-6]$/.test(tag)) {
    return ['editor-heading', 'editor-heading--'];
  }
  return ['editor-text'];
}


const files = filesBelow(sourceRoot);
const canonicalMenu = path.join(sourceRoot, 'editor', 'ContextMenu.tsx');
const canonicalDocker = path.join(sourceRoot, 'editor', 'DockerList.tsx');
const privateDockerListClass = /(?:^|[\s"'`])(?:docker-list-layout|docker-list-stack|docker-list-header|docker-item-list|docker-item|docker-item-select|docker-list-footer)(?=$|[\s"'`])/;
const registryPath = path.join(root, 'docs/widgets.md');
const registry = fs.readFileSync(registryPath, 'utf8');
for (const source of ['ContextMenu.tsx', 'DockerList.tsx', 'DockableWorkspace.tsx', 'EditorDialogShell.tsx', 'itemReorder.tsx', 'NumberField.tsx', 'RecordSelect.tsx', 'NativeWindowControls.tsx']) {
  if (!fs.existsSync(path.join(sourceRoot, 'editor', source))) violations.push(`Missing registered widget: ${source}`);
  if (!registry.includes(`src/editor/${source}`)) violations.push(`Widget registry omits ${source}`);
}
for (const file of files.filter(file => file.endsWith('.tsx'))) {
  const source = fs.readFileSync(file, 'utf8');
  const relative = path.relative(root, file);
  if (file !== canonicalDocker && privateDockerListClass.test(source)) violations.push(`${relative} duplicates DockerList markup`);
  if (file !== canonicalMenu && (/\brole\s*=\s*["']menu["']/.test(source) || source.includes('editor-context-menu'))) violations.push(`${relative} defines a custom menu outside ContextMenu`);
  if (file === canonicalMenu && !(/\brole\s*=\s*["']menu["']/.test(source) && source.includes('editor-context-menu'))) violations.push('ContextMenu no longer owns the custom menu implementation');
  for (const match of source.matchAll(/<([a-z][a-z0-9]*)\b/g)) {
    const tag = match[1];
    if (!auditedTags.has(tag)) continue;
    const end = openingEnd(source, match.index + match[0].length);
    const line = source.slice(0, match.index).split('\n').length;
    if (end < 0) { violations.push(`${relative}:${line} unclosed <${tag}>`); continue; }
    const opening = source.slice(match.index, end + 1);
    // Feature classes on semantic controls/text inherit that role even when
    // the stylesheet selector omits the tag or shared class.
    const literalClass = opening.match(/className\s*=\s*["']([^"']*)["']/);
    for (const name of (literalClass?.[1] ?? '').split(/\s+/)) {
      if (name) semanticClasses.add(name);
    }
    counts.set(tag, (counts.get(tag) ?? 0) + 1);
    for (const required of requiredClasses(tag)) {
      if (!opening.includes(required)) violations.push(`${relative}:${line} <${tag}> lacks ${required}`);
    }
    if (opening.includes('editor-control--unstyled')) violations.push(`${relative}:${line} unregistered unstyled control`);
  }
}

// Feature styles own placement. Shared widget sources own reusable appearance.
// Compile Sass in memory so nested feature selectors cannot evade the audit.
const sharedStyles = new Set(['editor.scss', '_widget-structure.scss', '_editor-button-theme.scss', '_editor-control-theme.scss', '_editor-typography-theme.scss', 'editor-buttons.scss', 'editor-controls.scss', 'editor-typography.scss', 'editor-surfaces.scss'].map(name => path.join(sourceRoot, 'styles', name)));
const appearance = new Set(['color', 'background', 'background-color', 'border', 'border-color', 'border-radius', 'box-shadow', 'font', 'font-family', 'font-size', 'font-style', 'font-weight', 'line-height', 'letter-spacing', 'text-transform', 'outline', 'outline-color', 'accent-color', 'caret-color']);
for (const file of files.filter(file => /\.(?:scss|css)$/.test(file) && !sharedStyles.has(file))) {
  const css = file.endsWith('.scss') ? sass.compile(file).css : fs.readFileSync(file, 'utf8');
  for (const match of css.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selector = match[1].trim();
    const ordinary = /(?:^|[\s>,:+~(])(?:button|input|select|textarea|label|legend|h[1-6]|p|small|code|pre)(?=$|[\s[.#>:,+~)])/;
    const usesSemanticClass = [...selector.matchAll(/\.([\w-]+)/g)].some(match => semanticClasses.has(match[1]));
    if (!ordinary.test(selector) && !usesSemanticClass && !/\.editor-(?:button|control|field|heading|text)/.test(selector)) continue;
    for (const declaration of match[2].split(';')) {
      const property = declaration.slice(0, declaration.indexOf(':')).trim();
      if (appearance.has(property)) violations.push(`${path.relative(root, file)} repeats shared ${property} in ${selector.replace(/\s+/g, ' ')}`);
    }
  }
}
console.log(`Studio design-system audit: ${[...counts].map(([tag, count]) => `${tag}=${count}`).join(' ')}`);
if (violations.length) { console.error(violations.join('\n')); process.exitCode = 1; }
else console.log('Studio design-system source audit passed.');
