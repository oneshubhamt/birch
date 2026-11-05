import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = relative => fs.readFileSync(path.join(root, relative), 'utf8');
const spec = JSON.parse(read('editor-spec/document-spec.json'));

function exact(label, left, right) {
  const a = [...new Set(left)].sort();
  const b = [...new Set(right)].sort();
  if (a.length !== b.length || a.some((value, index) => value !== b[index])) {
    throw new Error(`${label} mismatch: actual=[${a}] expected=[${b}]`);
  }
}

function assertUnique(label, values) {
  if (new Set(values).size !== values.length) throw new Error(`${label} contains duplicates`);
}

function allowedKeys(label, value, allowed) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) throw new Error(`${label} contains unknown field ${key}`);
  }
}

allowedKeys('document spec', spec, new Set(['spec_version', 'envelope', 'contexts', 'domains', 'limits', 'nodes', 'marks']));
if (spec.spec_version !== 1) throw new Error('Unsupported Rustnote document-spec version');
for (const key of ['schema_version', 'editor_engine', 'document_schema', 'document_schema_version']) {
  if (!(key in spec.envelope)) throw new Error(`Document spec envelope is missing ${key}`);
}
if (!spec.nodes?.doc || !spec.nodes?.text || !spec.marks) throw new Error('Document spec is incomplete');

for (const [name, domain] of Object.entries(spec.domains || {})) {
  if (!Array.isArray(domain) || domain.length === 0) throw new Error(`Domain ${name} must be non-empty`);
  assertUnique(`domain ${name}`, domain);
}
for (const [name, value] of Object.entries(spec.limits || {})) {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`Limit ${name} must be a non-negative safe integer`);
}

const noteDomain = read('src/domain/note.rs');
const noteIdMaxMatch = noteDomain.match(/impl NoteId \{[\s\S]*?pub const MAX_LEN: usize = ([0-9_]+);/);
if (!noteIdMaxMatch) throw new Error('Cannot locate the Rust NoteId domain length contract');
const noteIdMaxBytes = Number(noteIdMaxMatch[1].replaceAll('_', ''));
if (spec.limits.note_id_max_bytes !== noteIdMaxBytes) {
  throw new Error(`Editor note-ID limit ${spec.limits.note_id_max_bytes} drifts from Rust NoteId::MAX_LEN ${noteIdMaxBytes}`);
}

const ruleKeys = new Set([
  'validator', 'invariants', 'contexts', 'attrs', 'attr_policy', 'allowed_marks',
  'list_item_type', 'list_item_context', 'first_child_type', 'child_context',
  'child_types', 'content_min', 'markless_contexts', 'mutually_exclusive_marks',
  'max_one_of_marks', 'item_attrs', 'media_annotation_attrs', 'unique_attr',
  'time_based_kind_domain', 'media_annotation_sort_key', 'prosemirror',
]);
const attrKeys = new Set([
  'type', 'nullable', 'required', 'enum_domain', 'min', 'max', 'min_limit', 'max_limit',
  'max_bytes_limit', 'max_items_limit', 'trim_non_empty', 'non_empty', 'no_control_chars',
  'trim_stable', 'const', 'default',
]);
const attrGroups = [];
const invariantClasses = new Map();

for (const [kind, rules] of [['node', spec.nodes], ['mark', spec.marks]]) {
  for (const [name, rule] of Object.entries(rules)) {
    allowedKeys(`${kind} ${name}`, rule, ruleKeys);
    if (typeof rule.validator !== 'string' || !rule.validator) {
      throw new Error(`${kind} ${name} has no validator class`);
    }
    if (!Array.isArray(rule.invariants)) throw new Error(`${kind} ${name} has no invariant contract`);
    assertUnique(`${kind} ${name} invariants`, rule.invariants);
    const classKey = `${kind}:${rule.validator}`;
    if (invariantClasses.has(classKey)) {
      exact(`${classKey} invariant assignment`, rule.invariants, invariantClasses.get(classKey));
    } else {
      invariantClasses.set(classKey, rule.invariants);
    }
    if (!['strict', 'known', 'passthrough'].includes(rule.attr_policy)) {
      throw new Error(`${kind} ${name} has invalid attr_policy ${rule.attr_policy}`);
    }
    if (rule.prosemirror !== undefined) {
      allowedKeys(`${kind} ${name}.prosemirror`, rule.prosemirror, new Set(['content', 'group', 'inline', 'marks', 'excludes']));
      for (const key of ['content', 'group', 'marks', 'excludes']) {
        if (rule.prosemirror[key] !== undefined && typeof rule.prosemirror[key] !== 'string') {
          throw new Error(`${kind} ${name}.prosemirror.${key} must be a string`);
        }
      }
      if (rule.prosemirror.inline !== undefined && typeof rule.prosemirror.inline !== 'boolean') {
        throw new Error(`${kind} ${name}.prosemirror.inline must be a boolean`);
      }
    }
    if (kind === 'node') {
      if (!Array.isArray(rule.contexts) || rule.contexts.length === 0) {
        throw new Error(`node ${name} has no document contexts`);
      }
      if (rule.child_context !== undefined && typeof rule.child_context !== 'string') {
        throw new Error(`node ${name} has invalid child_context`);
      }
      if (rule.unique_attr !== undefined
          && !Object.prototype.hasOwnProperty.call(rule.attrs || {}, rule.unique_attr)) {
        throw new Error(`node ${name} unique_attr is not a declared attribute`);
      }
      if (rule.time_based_kind_domain !== undefined
          && !Array.isArray(spec.domains[rule.time_based_kind_domain])) {
        throw new Error(`node ${name} references missing time-based kind domain`);
      }
      if (rule.media_annotation_sort_key !== undefined
          && (!Array.isArray(rule.media_annotation_sort_key) || rule.media_annotation_sort_key.length === 0)) {
        throw new Error(`node ${name} has invalid media annotation sort key`);
      }
    }
    attrGroups.push([`${kind}.${name}.attrs`, rule.attrs || {}]);
    if (rule.item_attrs) attrGroups.push([`${kind}.${name}.item_attrs`, rule.item_attrs]);
    if (rule.media_annotation_attrs) {
      attrGroups.push([`${kind}.${name}.media_annotation_attrs`, rule.media_annotation_attrs]);
    }
  }
}

if (!Array.isArray(spec.contexts) || spec.contexts.length === 0) throw new Error('Document spec has no context vocabulary');
assertUnique('document contexts', spec.contexts);
const declaredContexts = new Set(spec.contexts);
for (const [name, rule] of Object.entries(spec.nodes)) {
  for (const context of rule.contexts || []) {
    if (!declaredContexts.has(context)) throw new Error(`node ${name} references unknown context ${context}`);
  }
  for (const context of rule.markless_contexts || []) {
    if (!declaredContexts.has(context)) throw new Error(`node ${name} references unknown markless context ${context}`);
  }
  if (rule.child_context && !declaredContexts.has(rule.child_context)) {
    throw new Error(`node ${name} references unknown child context ${rule.child_context}`);
  }
  for (const childType of rule.child_types || []) {
    if (!spec.nodes[childType]) throw new Error(`node ${name} references unknown child type ${childType}`);
  }
  if (rule.list_item_type && !spec.nodes[rule.list_item_type]) {
    throw new Error(`node ${name} references unknown list item type ${rule.list_item_type}`);
  }
  if (rule.list_item_context && !declaredContexts.has(rule.list_item_context)) {
    throw new Error(`node ${name} references unknown list item context ${rule.list_item_context}`);
  }
  if (rule.first_child_type && !spec.nodes[rule.first_child_type]) {
    throw new Error(`node ${name} references unknown first child type ${rule.first_child_type}`);
  }
  for (const mark of rule.allowed_marks || []) {
    if (!spec.marks[mark]) throw new Error(`node ${name} references unknown allowed mark ${mark}`);
  }
  for (const group of rule.mutually_exclusive_marks || []) {
    for (const mark of group) {
      if (!spec.marks[mark]) throw new Error(`node ${name} references unknown exclusive mark ${mark}`);
    }
  }
  for (const mark of rule.max_one_of_marks || []) {
    if (!spec.marks[mark]) throw new Error(`node ${name} references unknown limited mark ${mark}`);
  }
}

for (const [owner, attrs] of attrGroups) {
  if (!attrs || typeof attrs !== 'object' || Array.isArray(attrs)) {
    throw new Error(`${owner} must be an object`);
  }
  for (const [name, attr] of Object.entries(attrs)) {
    allowedKeys(`${owner}.${name}`, attr, attrKeys);
    if (!['string', 'integer', 'boolean', 'array'].includes(attr.type)) {
      throw new Error(`${owner}.${name} has unsupported type ${attr.type}`);
    }
    if (attr.enum_domain && !Array.isArray(spec.domains[attr.enum_domain])) {
      throw new Error(`${owner}.${name} references missing domain ${attr.enum_domain}`);
    }
    for (const ref of ['min_limit', 'max_limit', 'max_bytes_limit', 'max_items_limit']) {
      if (attr[ref] && !Number.isSafeInteger(spec.limits[attr[ref]])) {
        throw new Error(`${owner}.${name} references missing limit ${attr[ref]}`);
      }
    }
    if (Number.isSafeInteger(attr.min) && Number.isSafeInteger(attr.max) && attr.min > attr.max) {
      throw new Error(`${owner}.${name} has inverted integer bounds`);
    }
  }
}

function blockKeys(source, marker) {
  const start = source.indexOf(marker);
  if (start < 0) throw new Error(`Missing source marker ${marker}`);
  const tail = source.slice(start + marker.length);
  const end = tail.indexOf('\n});');
  const endPlain = tail.indexOf('\n};');
  const boundary = end >= 0 && (endPlain < 0 || end < endPlain) ? end : endPlain;
  if (boundary < 0) throw new Error(`Cannot parse source block ${marker}`);
  return [...tail.slice(0, boundary).matchAll(/^  ([a-z][a-z0-9_]*)(?:\([^\n]*\))?\s*[:(]/gm)]
    .map(match => match[1]);
}

const schema = read('editor-src/document/schema.js');
const validation = read('editor-src/document/validation.js');
const nodeImplementationKeys = blockKeys(schema, 'const nodeImplementations = {');
if (schema.includes('...tableNodeSpecs')) {
  nodeImplementationKeys.push(...Object.entries(spec.nodes)
    .filter(([, rule]) => ['table', 'table_row', 'table_cell'].includes(rule.validator))
    .map(([name]) => name));
}
exact('ProseMirror node vocabulary', nodeImplementationKeys, Object.keys(spec.nodes));
exact('ProseMirror mark vocabulary', blockKeys(schema, 'const markImplementations = {'), Object.keys(spec.marks));
exact('JS node validator classes', blockKeys(validation, 'const NODE_VALIDATORS = Object.freeze({'), Object.values(spec.nodes).map(rule => rule.validator));
exact('JS mark validator classes', blockKeys(validation, 'const MARK_VALIDATORS = Object.freeze({'), Object.values(spec.marks).map(rule => rule.validator));
exact('JS node invariant classes', blockKeys(validation, 'const NODE_VALIDATOR_INVARIANTS = Object.freeze({'), Object.values(spec.nodes).map(rule => rule.validator));
exact('JS mark invariant classes', blockKeys(validation, 'const MARK_VALIDATOR_INVARIANTS = Object.freeze({'), Object.values(spec.marks).map(rule => rule.validator));

function rustValidatorClasses(source) {
  const blocks = [...source.matchAll(/match rule\.validator\.as_str\(\) \{([\s\S]*?)\n    \}/g)]
    .map(match => match[1]);
  if (blocks.length < 2) throw new Error('Cannot locate independent Rust node/mark validator dispatchers');
  return blocks.map(block => [...block.matchAll(/^        ((?:"[a-z_]+"(?: \| )?)+) =>/gm)]
    .flatMap(match => [...match[1].matchAll(/"([a-z_]+)"/g)].map(value => value[1])));
}

function rustInvariantClasses(source, functionName) {
  const start = source.indexOf(`fn ${functionName}(`);
  if (start < 0) throw new Error(`Cannot locate Rust ${functionName}`);
  const match = source.slice(start).match(/match validator \{([\s\S]*?)\n        _ => None,/);
  if (!match) throw new Error(`Cannot parse Rust ${functionName}`);
  return [...match[1].matchAll(/^        ((?:"[a-z_]+"(?: \| )?)+) =>/gm)]
    .flatMap(value => [...value[1].matchAll(/"([a-z_]+)"/g)].map(name => name[1]));
}

const rustDocument = read('src/editor_document.rs');
const [rustNodeValidators, rustMarkValidators] = rustValidatorClasses(rustDocument);
exact('Rust node validator classes', rustNodeValidators, Object.values(spec.nodes).map(rule => rule.validator));
exact('Rust mark validator classes', rustMarkValidators, Object.values(spec.marks).map(rule => rule.validator));
exact('Rust node invariant classes', rustInvariantClasses(rustDocument, 'node_validator_invariants'), Object.values(spec.nodes).map(rule => rule.validator));
exact('Rust mark invariant classes', rustInvariantClasses(rustDocument, 'mark_validator_invariants'), Object.values(spec.marks).map(rule => rule.validator));
const rustContextBlock = rustDocument.slice(
  rustDocument.indexOf('fn as_spec_name(self)'),
  rustDocument.indexOf('fn from_spec_name(name: &str)'),
);
const rustContexts = [...rustContextBlock.matchAll(/=> "([a-z_]+)"/g)].map(match => match[1]);
exact('Rust NodeContext vocabulary', rustContexts, spec.contexts);

const rustSpec = read('src/editor_document_spec.rs');
for (const marker of [
  'include_str!("../editor-spec/document-spec.json")',
  '#[serde(deny_unknown_fields)]',
  'pub(crate) invariants: Vec<String>',
  'Some(NoteId::MAX_LEN as u64)',
]) {
  if (!rustSpec.includes(marker)) throw new Error(`Rust spec adapter is missing contract: ${marker}`);
}

const config = read('editor-src/config.js');
for (const marker of [
  'ENVELOPE_SPEC.editor_engine', 'ENVELOPE_SPEC.schema_version',
  'ENVELOPE_SPEC.document_schema', 'ENVELOPE_SPEC.document_schema_version',
]) {
  if (!schema.includes(marker)) throw new Error(`Editor envelope construction is not spec-derived: ${marker}`);
}
for (const forbidden of [
  'const EDITOR_DOCUMENT_SCHEMA_VERSION', 'const DOCUMENT_SCHEMA_VERSION: u32',
  'const EDITOR_ENGINE: &str', 'const DOCUMENT_SCHEMA: &str', 'switch (nodeType)',
  'switch (markType)',
]) {
  if (rustDocument.includes(forbidden) || validation.includes(forbidden)) {
    throw new Error(`Parallel document authority survived: ${forbidden}`);
  }
}
for (const forbidden of [
  'const EDITOR_ENGINE =', 'const ENVELOPE_SCHEMA_VERSION =',
  'const DOCUMENT_SCHEMA =', 'const DOCUMENT_SCHEMA_VERSION =',
]) {
  if (config.includes(forbidden)) throw new Error(`Derived envelope alias survived in config: ${forbidden}`);
}

const build = read('scripts/build-prosemirror-editor.mjs');
const runtimeInputs = JSON.parse(read('editor-src/runtime-inputs.json'));
for (const requiredInput of [
  'editor-spec/document-spec.json',
  'editor-src/document/spec.js',
  'editor-src/document/validation.js',
]) {
  if (!runtimeInputs.editor_source_inputs?.includes(requiredInput)) {
    throw new Error(`Canonical editor runtime inputs omit document-spec contract: ${requiredInput}`);
  }
}
for (const marker of [
  "canonicalRuntimeInputs('editor_source_inputs')",
  'documentEnvelope.editor_engine', 'documentEnvelope.schema_version',
  'documentEnvelope.document_schema', 'documentEnvelope.document_schema_version',
]) {
  if (!build.includes(marker)) throw new Error(`Editor build does not include canonical spec contract: ${marker}`);
}
for (const asset of ['assets/blank-editor-state.json', 'assets/welcome-note-editor-state.json']) {
  const state = JSON.parse(read(asset));
  for (const [key, value] of Object.entries(spec.envelope)) {
    if (state[key] !== value) throw new Error(`${asset} envelope drifts from document spec at ${key}`);
  }
}

console.log(`Canonical editor document specification validated: ${Object.keys(spec.nodes).length} nodes, ${Object.keys(spec.marks).length} marks.`);
