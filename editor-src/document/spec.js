import documentSpecSource from '../../editor-spec/document-spec.json' with {type: 'json'};

function deepFreeze(value) {
  if (!value || typeof value !== 'object' || Object.isFrozen(value)) return value;
  Object.freeze(value);
  for (const child of Object.values(value)) deepFreeze(child);
  return value;
}

const DOCUMENT_SPEC = deepFreeze(documentSpecSource);
const ENVELOPE_SPEC = DOCUMENT_SPEC.envelope;

function specDomain(name) {
  const domain = DOCUMENT_SPEC.domains[name];
  if (!Array.isArray(domain)) throw new Error(`Unknown Rustnote document-spec domain: ${name}`);
  return domain;
}

function specLimit(name) {
  const value = DOCUMENT_SPEC.limits[name];
  if (!Number.isSafeInteger(value)) throw new Error(`Unknown Rustnote document-spec limit: ${name}`);
  return value;
}

function nodeRule(name) {
  const rule = DOCUMENT_SPEC.nodes[name];
  if (!rule) throw new Error(`Unknown Rustnote document node: ${name}`);
  return rule;
}

function nodeNamesForValidator(validator) {
  return Object.freeze(Object.entries(DOCUMENT_SPEC.nodes)
    .filter(([, rule]) => rule.validator === validator)
    .map(([name]) => name));
}

function markRule(name) {
  const rule = DOCUMENT_SPEC.marks[name];
  if (!rule) throw new Error(`Unknown Rustnote document mark: ${name}`);
  return rule;
}


function utf8Bytes(value) {
  const bytes = [];
  for (let index = 0; index < value.length; index += 1) {
    let code = value.charCodeAt(index);
    if (code <= 0x7f) {
      bytes.push(code);
    } else if (code <= 0x7ff) {
      bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
    } else if (code >= 0xd800 && code <= 0xdbff
        && index + 1 < value.length
        && value.charCodeAt(index + 1) >= 0xdc00
        && value.charCodeAt(index + 1) <= 0xdfff) {
      const low = value.charCodeAt(index + 1);
      code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
      bytes.push(
        0xf0 | (code >> 18),
        0x80 | ((code >> 12) & 0x3f),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
      index += 1;
    } else {
      bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    }
  }
  return bytes;
}

function utf8ByteLength(value) {
  return utf8Bytes(value).length;
}

function truncateUtf8(value, maxBytes) {
  const text = String(value ?? '');
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 0) {
    throw new Error(`Invalid Rustnote UTF-8 byte limit: ${String(maxBytes)}`);
  }
  if (utf8ByteLength(text) <= maxBytes) return text;
  let output = '';
  let used = 0;
  for (const character of text) {
    const bytes = utf8ByteLength(character);
    if (used + bytes > maxBytes) break;
    output += character;
    used += bytes;
  }
  return output;
}

function utf8StringCompare(left, right) {
  const leftBytes = utf8Bytes(left);
  const rightBytes = utf8Bytes(right);
  const length = Math.min(leftBytes.length, rightBytes.length);
  for (let index = 0; index < length; index += 1) {
    if (leftBytes[index] !== rightBytes[index]) return leftBytes[index] < rightBytes[index] ? -1 : 1;
  }
  if (leftBytes.length === rightBytes.length) return 0;
  return leftBytes.length < rightBytes.length ? -1 : 1;
}

function resolvedIntegerBound(attr, directKey, limitKey) {
  if (Number.isSafeInteger(attr[directKey])) return attr[directKey];
  if (typeof attr[limitKey] === 'string') return specLimit(attr[limitKey]);
  return null;
}

function validateSpecAttributeValue(value, attr, path) {
  if (value === null) {
    if (attr.nullable) return;
    throw new Error(`${path} cannot be null`);
  }
  if (attr.type === 'string') {
    if (typeof value !== 'string') throw new Error(`${path} must be a string`);
    const byteLength = utf8ByteLength(value);
    if (attr.trim_non_empty && value.trim() === '') throw new Error(`${path} must not be blank`);
    if (attr.trim_stable && value.trim() !== value) throw new Error(`${path} must not contain surrounding whitespace`);
    if (attr.non_empty && value.length === 0) throw new Error(`${path} must not be empty`);
    if (attr.no_control_chars && /[\u0000-\u001f\u007f-\u009f]/u.test(value)) throw new Error(`${path} contains a control character`);
    const maxBytes = typeof attr.max_bytes_limit === 'string' ? specLimit(attr.max_bytes_limit) : null;
    if (maxBytes !== null && byteLength > maxBytes) throw new Error(`${path} exceeds its byte limit`);
    if (typeof attr.enum_domain === 'string' && !specDomain(attr.enum_domain).includes(value)) {
      throw new Error(`${path} is outside its allowed domain`);
    }
  } else if (attr.type === 'integer') {
    if (!Number.isSafeInteger(value)) throw new Error(`${path} must be a safe integer`);
    const min = resolvedIntegerBound(attr, 'min', 'min_limit');
    const max = resolvedIntegerBound(attr, 'max', 'max_limit');
    if (min !== null && value < min) throw new Error(`${path} is below its minimum`);
    if (max !== null && value > max) throw new Error(`${path} exceeds its maximum`);
  } else if (attr.type === 'boolean') {
    if (typeof value !== 'boolean') throw new Error(`${path} must be a boolean`);
  } else if (attr.type === 'array') {
    if (!Array.isArray(value)) throw new Error(`${path} must be an array`);
    const maxItems = typeof attr.max_items_limit === 'string' ? specLimit(attr.max_items_limit) : null;
    if (maxItems !== null && value.length > maxItems) throw new Error(`${path} contains too many items`);
  } else {
    throw new Error(`Unsupported Rustnote document-spec attribute type: ${String(attr.type)}`);
  }
  if (Object.prototype.hasOwnProperty.call(attr, 'const') && value !== attr.const) {
    throw new Error(`${path} has an unsupported fixed value`);
  }
}

function validateAttributeObject(attrs, rules, policy, path) {
  if (!attrs || typeof attrs !== 'object' || Array.isArray(attrs)) {
    throw new Error(`${path} must be a JSON object`);
  }
  if (policy === 'strict') {
    for (const key of Object.keys(attrs)) {
      if (!Object.prototype.hasOwnProperty.call(rules, key)) throw new Error(`${path}.${key} is unsupported`);
    }
  }
  if (policy !== 'passthrough') {
    for (const [name, attr] of Object.entries(rules)) {
      if (!Object.prototype.hasOwnProperty.call(attrs, name)) {
        if (attr.required) throw new Error(`${path}.${name} is required`);
        continue;
      }
      validateSpecAttributeValue(attrs[name], attr, `${path}.${name}`);
    }
  }
  return attrs;
}

function validateSpecAttrs(owner, path) {
  const rules = owner.rule.attrs || {};
  const policy = owner.rule.attr_policy || 'passthrough';
  const hasAttrs = Object.prototype.hasOwnProperty.call(owner.value, 'attrs');
  if (policy === 'passthrough') {
    return hasAttrs ? owner.value.attrs : null;
  }
  if (!hasAttrs) {
    for (const [name, attr] of Object.entries(rules)) {
      if (attr.required) throw new Error(`${path}.attrs.${name} is required`);
    }
    return null;
  }
  return validateAttributeObject(owner.value.attrs, rules, policy, `${path}.attrs`);
}

function contextAllowed(rule, context) {
  return Array.isArray(rule.contexts) && rule.contexts.includes(context);
}

function nodeAttrDefault(nodeName, attrName) {
  const attr = nodeRule(nodeName).attrs?.[attrName];
  if (!attr || !Object.prototype.hasOwnProperty.call(attr, 'default')) {
    throw new Error(`Rustnote document node ${nodeName}.${attrName} has no canonical default`);
  }
  return attr.default;
}

function markAttrDefault(markName, attrName) {
  const attr = markRule(markName).attrs?.[attrName];
  if (!attr || !Object.prototype.hasOwnProperty.call(attr, 'default')) {
    throw new Error(`Rustnote document mark ${markName}.${attrName} has no canonical default`);
  }
  return attr.default;
}

function prosemirrorAttrs(rule) {
  if (!rule.attrs) return undefined;
  return Object.fromEntries(Object.entries(rule.attrs).map(([name, attr]) => [
    name,
    Object.prototype.hasOwnProperty.call(attr, 'default') ? {default: attr.default} : {},
  ]));
}

function withNodeContract(name, implementation) {
  const rule = nodeRule(name);
  const contract = rule.prosemirror || {};
  const attrs = prosemirrorAttrs(rule);
  return {
    ...implementation,
    ...contract,
    ...(attrs ? {attrs} : {}),
  };
}

function withMarkContract(name, implementation) {
  const rule = markRule(name);
  const contract = rule.prosemirror || {};
  const attrs = prosemirrorAttrs(rule);
  return {
    ...implementation,
    ...contract,
    ...(attrs ? {attrs} : {}),
  };
}

function assertExactImplementationVocabulary(kind, implementations, rules) {
  const implemented = Object.keys(implementations).sort();
  const specified = Object.keys(rules).sort();
  if (implemented.length !== specified.length
      || implemented.some((name, index) => name !== specified[index])) {
    throw new Error(`Rustnote ${kind} implementation vocabulary does not match document-spec authority`);
  }
}

export {
  DOCUMENT_SPEC,
  ENVELOPE_SPEC,
  assertExactImplementationVocabulary,
  validateAttributeObject,
  contextAllowed,
  markAttrDefault,
  markRule,
  nodeAttrDefault,
  nodeNamesForValidator,
  nodeRule,
  specDomain,
  specLimit,
  truncateUtf8,
  utf8ByteLength,
  utf8StringCompare,
  validateSpecAttrs,
  withMarkContract,
  withNodeContract,
};
