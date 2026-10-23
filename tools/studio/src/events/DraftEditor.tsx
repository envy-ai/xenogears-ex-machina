/** Authored instruction controls. Shared Rust transactions own all byte changes. */
import { useEffect, useMemo, useState } from 'react';
import type { Backend } from '../backend/service';
import { readCommandResult } from '../backend/service';
import type { DraftAction, DraftSnapshot, NodeInput, OperandValue } from '../backend/drafts';
import { formatPc, instructionPhrase, type Instruction, type NameTable } from './model';

interface Template { bytes: number[]; instruction: Instruction; form: number }
export interface DraftEditorProps {
  backend: Backend; snapshot: DraftSnapshot; pc: number | null; actor: number; event: number;
  map: number; names: NameTable; mode: 'operand' | 'insert' | 'event';
  disabled?: boolean;
  onApply(action: DraftAction): void; onCancel(): void; onDirty(value: boolean): void;
  onError(error: unknown): void;
}
interface OperandDraft { text: string; immediate: boolean | null; target: string }
const integer = (text: string): number => {
  if (!/^(?:0[xX][0-9a-fA-F]+|[0-9]+)$/.test(text.trim())) throw new Error('Enter a nonnegative integer or 0x hexadecimal value');
  const value = Number(text); if (!Number.isSafeInteger(value) || value > 0xffff_ffff) throw new Error('Operand must fit an unsigned 32-bit value');
  return value;
};
export function DraftEditor(props: DraftEditorProps) {
  const { snapshot, pc, mode } = props;
  const selected = snapshot.graph.instructions.find(ins => ins.pc === pc);
  const selectedId = Object.entries(snapshot.bindings).find(([, at]) => at === pc)?.[0];
  const [templates, setTemplates] = useState<Template[]>([]);
  const [template, setTemplate] = useState('');
  const [fields, setFields] = useState<OperandDraft[]>([]);
  const [eventActor, setEventActor] = useState(String(props.actor));
  const [eventSlot, setEventSlot] = useState(String(props.event));
  const [eventEnding, setEventEnding] = useState('end');
  const [failure, setFailure] = useState('');
  const [dirty, setDirty] = useState(false);
  const chosen = mode === 'operand' ? selected : templates[Number(template)]?.instruction;
  const bindings = useMemo(() => Object.entries(snapshot.bindings).filter(([, at]) => snapshot.graph.instructions.some(ins => ins.pc === at)), [snapshot]);
  useEffect(() => {
    try {
      const result = readCommandResult<{ templates: Template[] }>(props.backend.command(JSON.stringify({ op: 'field.templates' })));
      setTemplates(result.templates);
      const nop = result.templates.findIndex(item => item.instruction.opcode === 0x0e && item.instruction.extended === null);
      setTemplate(String(nop >= 0 ? nop : 0));
    } catch (error) { setFailure(String(error)); props.onError(error); }
    // The backend remains stable for an editor lifecycle.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.backend]);
  useEffect(() => {
    setFields(chosen?.spec.operands.map((descriptor, index) => ({
      text: String(chosen.operands[index]), immediate: chosen.immediate[index],
      target: descriptor.kind === 'addr' ? bindings.find(([, at]) => at === chosen.operands[index])?.[0] ?? '' : '',
    })) ?? []);
    setFailure(''); setDirty(false); props.onDirty(mode !== 'operand');
    // Explicit selection/transaction changes reset only after the owner guard.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chosen, selectedId, snapshot.revision, mode]);
  const change = (index: number, update: Partial<OperandDraft>) => {
    setFields(value => value.map((field, at) => at === index ? { ...field, ...update } : field));
    setDirty(true); props.onDirty(true); setFailure('');
  };
  const operandValue = (index: number): OperandValue => {
    const field = fields[index]; const descriptor = chosen!.spec.operands[index];
    if (descriptor.kind === 'addr') {
      if (!field.target) throw new Error(`Operand ${index + 1}: select an instruction target`);
      return { kind: 'target', node: field.target };
    }
    return { kind: 'literal', value: integer(field.text), ...(field.immediate === null ? {} : { immediate: field.immediate }) };
  };
  const apply = () => {
    try {
      if (props.disabled) throw new Error('Wait for the pending map/project operation before applying fields');
      if (!chosen) throw new Error('Select an instruction');
      const values = fields.map((_, index) => operandValue(index));
      if (mode === 'operand') {
        if (!selectedId) throw new Error('Compiler scaffolding is not an editable authored node');
        const actions = values.flatMap((value, operand) => {
          const original = chosen.spec.operands[operand].kind === 'addr' ? bindings.find(([, at]) => at === chosen.operands[operand])?.[0] ?? '' : null;
          const changed = value.kind === 'target' ? value.node !== original : value.value !== chosen.operands[operand] || (value.immediate !== undefined && value.immediate !== chosen.immediate[operand]);
          return changed ? [{ kind: 'set_operand' as const, node: selectedId, operand, value }] : [];
        });
        if (actions.length) props.onApply({ kind: 'batch', actions });
      } else {
        const source = templates[Number(template)];
        if (!source) throw new Error('Select a supported instruction template');
        const node = readCommandResult<NodeInput>(props.backend.command(JSON.stringify({ op: 'field.instruction', args: { bytes: source.bytes, operands: values } })));
        if (mode === 'insert') {
          if (!selectedId) throw new Error('Select an authored insertion anchor');
          props.onApply({ kind: 'insert_after', after: selectedId, ...node });
        } else {
          const ending = templates.find(item => item.instruction.spec.flow === eventEnding && item.instruction.spec.operands.length === 0);
          if (!ending) throw new Error('The selected event ending is unavailable in the opcode schema');
          props.onApply({ kind: 'new_event', actor: integer(eventActor), event: integer(eventSlot), nodes: [node, { bytes: ending.bytes }] });
        }
      }
      props.onDirty(false); setDirty(false); setFailure('');
    } catch (error) { setFailure(error instanceof Error ? error.message : String(error)); props.onError(error); }
  };
  return <section className="event-draft-editor editor-inset-frame" aria-label="Authored instruction editor" onKeyDown={event => {
    if (props.disabled) return;
    if (event.key === 'Escape') { event.preventDefault(); props.onCancel(); }
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); apply(); }
  }}>
    <div className="event-detail-heading"><h3 className="editor-heading editor-heading--section">{mode === 'operand' ? 'Instruction' : mode === 'insert' ? 'Insert node after selection' : 'Create event in existing actor slot'} · Authored draft</h3><span className="editor-text editor-text--code">{selectedId ?? 'No authored selection'}</span></div>
    {mode === 'event' && <div className="event-draft-fields">
      <label className="editor-field editor-field-label"><span>Existing actor ID</span><input className="editor-control editor-control--text" disabled={props.disabled} value={eventActor} onChange={event => { setEventActor(event.target.value); props.onDirty(true); }} /></label>
      <label className="editor-field editor-field-label"><span>Unused or terminal-only event slot (0–31)</span><input className="editor-control editor-control--text" disabled={props.disabled} value={eventSlot} onChange={event => { setEventSlot(event.target.value); props.onDirty(true); }} /></label>
      <label className="editor-field editor-field-label"><span>Event ending</span><select className="editor-control editor-control--select" disabled={props.disabled} value={eventEnding} onChange={event => { setEventEnding(event.target.value); props.onDirty(true); }}><option value="end">End slot</option><option value="return">Return to caller</option></select></label>
    </div>}
    {mode === 'operand' ? <p className="editor-text editor-text--code">{selected ? `${formatPc(selected.pc)} · ${selected.spec.mnemonic}` : 'Select an instruction to edit.'}</p> : <label className="editor-field editor-field-label"><span>Opcode · supported standalone form</span><select className="editor-control editor-control--select" disabled={props.disabled} value={template} onChange={event => { setTemplate(event.target.value); props.onDirty(true); }}>
      {templates.map((item, index) => <option value={index} key={index}>{item.instruction.extended === null ? '' : 'Extended '}{item.instruction.opcode.toString(16).padStart(2, '0')} · {item.instruction.spec.mnemonic} · form {item.form}</option>)}
    </select></label>}
    <p className="editor-text editor-text--caption">{chosen?.spec.effect}</p>
    <div className="event-draft-fields">{chosen?.spec.operands.map((descriptor, index) => <div className="event-draft-operand" key={index}>
      <label className="editor-field editor-field-label"><span>Operand {index + 1} · {descriptor.kind}</span>{descriptor.kind === 'addr'
        ? <select className="editor-control editor-control--select" disabled={props.disabled} value={fields[index]?.target ?? ''} onChange={event => change(index, { target: event.target.value })}><option value="">Select instruction target</option>{bindings.map(([id, at]) => <option value={id} key={id}>{id} · {instructionPhrase(snapshot.graph.instructions.find(ins => ins.pc === at)!, props.names, props.map, props.actor)}</option>)}</select>
        : <input className="editor-control editor-control--text" disabled={props.disabled} value={fields[index]?.text ?? ''} onChange={event => change(index, { text: event.target.value })} />}</label>
      {fields[index]?.immediate !== null && fields[index]?.immediate !== undefined && <label className="editor-field editor-field-label"><span>Operand {index + 1} mode</span><select className="editor-control editor-control--select" disabled={props.disabled} value={fields[index].immediate ? 'immediate' : 'variable'} onChange={event => change(index, { immediate: event.target.value === 'immediate' })}><option value="immediate">Immediate value</option><option value="variable">Variable byte reference</option></select></label>}
    </div>)}</div>
    {failure && <pre className="editor-text editor-text--danger" role="alert">{failure}</pre>}
    <div className="studio-toolbar"><button className="editor-button editor-button--primary" disabled={props.disabled || !chosen || (mode === 'operand' && (!dirty || !fields.length))} onClick={apply} title="Ctrl+Enter">Apply</button><button className="editor-button" disabled={props.disabled} onClick={props.onCancel} title="Escape">Cancel</button><span className="editor-text editor-text--caption">Changes affect the authored draft; live memory is separate.</span></div>
  </section>;
}
