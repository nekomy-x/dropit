/**
 * Developer surface — scoped API keys, usage and the in-panel documentation.
 *
 * Contract: ADR-010 (docs/adr/ADR-010-developer-api-v1.md) and the club panel
 * part of `docs/openapi.yaml`:
 *
 *   GET    /v1/club/api-keys                 → { items: ApiKeyView[] }
 *   POST   /v1/club/api-keys                 → 201 { ..ApiKeyView, key }  (secret once)
 *   GET    /v1/club/api-keys/scopes          → { scopes, presets }
 *   GET    /v1/club/api-keys/{id}            → ApiKeyView
 *   PATCH  /v1/club/api-keys/{id}            → ApiKeyView  (narrow scopes / rename)
 *   POST   /v1/club/api-keys/{id}/rotate     → 201 { ..ApiKeyView, key, replaces }
 *   DELETE /v1/club/api-keys/{id}            → 204
 *   GET    /v1/club/api-keys/{id}/usage      → { summary, items }
 *   GET    /v1/club/developers/docs          → manifest
 *   GET    /v1/club/developers/docs/{section}→ one section with markdown
 *   GET    /v1/club/developers/openapi.json  → OpenAPI 3.1 JSON
 *
 * Transport, auth and the 401 refresh/replay are NOT reimplemented here: every
 * call goes through `apiRequest`, the club client's one authenticated request,
 * so the in-memory Bearer token stays the only token this app handles.
 *
 * The plaintext key appears in exactly two responses — create and rotate — and
 * is never requested again. Nothing in this module caches it.
 */
import { apiRequest } from './client';

/** Live or test. A `test` key is refused on a deployment whose payments are live. */
export type ApiKeyEnvironment = 'live' | 'test';

/** `active` until revoked; `expired` is derived from `expires_at` on the server. */
export type ApiKeyStatus = 'active' | 'revoked';

/**
 * One key, as the API describes it. There is no secret field: the plaintext is
 * returned once by create/rotate and stored (hashed) on the server only.
 */
export interface ApiKeyView {
  id: string;
  name: string;
  /** Public lookup id (10 hex chars): safe in lists, logs and support tickets. */
  prefix: string;
  environment: ApiKeyEnvironment;
  /** Explicit, ordered scope ids such as `requests:read`. No wildcard exists. */
  scopes: string[];
  status: ApiKeyStatus;
  rate_limit_per_min: number;
  allowed_cidrs: string[];
  expires_at: string | null;
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
  rotated_from: string | null;
  /** Server-computed: `expires_at` has passed, whatever `status` says. */
  expired: boolean;
}

export interface ApiKeyListResponse {
  items: ApiKeyView[];
}

/** One scope from the server-side catalogue (the UI never invents scopes). */
export interface ScopeInfo {
  id: string;
  title: string;
  description: string;
  /** Checklist group: Documentation / Club / Live room / Orders / Catalog / Analytics. */
  group: string;
  /** `read`, `write` or `sensitive`. */
  risk: string;
  /** Documentation section that explains this scope. */
  docs_section: string;
}

export interface ScopePreset {
  id: string;
  title: string;
  description: string;
  scopes: string[];
}

export interface ScopeCatalogue {
  scopes: ScopeInfo[];
  presets: ScopePreset[];
}

export interface CreateApiKeyInput {
  name: string;
  scopes: string[];
  environment?: ApiKeyEnvironment;
  /** 1..365 days; omit for a key that never expires. */
  expires_in_days?: number;
  /** 30..600 requests per minute; omitting keeps the server default. */
  rate_limit_per_min?: number;
  /** Exact IPs or CIDR ranges (v4/v6). Empty means "any address". */
  allowed_cidrs?: string[];
}

/** Narrow-only patch: an existing key can lose scopes, never gain them. */
export interface PatchApiKeyInput {
  name?: string;
  scopes?: string[];
}

export interface CreatedApiKey extends ApiKeyView {
  /** Plaintext `dit_…`. Shown once, then gone. */
  key: string;
}

export interface RotatedApiKey extends CreatedApiKey {
  /** Id of the key whose secret this rotation replaces. */
  replaces: string;
}

export interface UsageSummary {
  calls_24h: number;
  errors_24h: number;
  calls_30d: number;
  last_call_at: string | null;
}

export interface UsageEntry {
  at: string;
  method: string;
  path: string;
  status: number;
}

export interface KeyUsage {
  summary: UsageSummary;
  items: UsageEntry[];
}

export interface DocsSectionMeta {
  id: string;
  title: string;
  summary: string;
  requires: string[];
  order: number;
}

export interface DocsManifest {
  api_version: string;
  /** `/v1/developer` — the base path a key calls, not the panel's path. */
  base_url: string;
  openapi_url: string;
  sections: DocsSectionMeta[];
  scopes: ScopeInfo[];
  presets: ScopePreset[];
}

export interface DocsSection extends DocsSectionMeta {
  markdown: string;
}

/** One operation of the OpenAPI document, flattened for the reference view. */
export interface OpenApiOperation {
  method: string;
  path: string;
  summary: string;
  description: string;
  operationId: string;
  tags: string[];
  /** `x-required-scope`: the scope a key must carry to call this operation. */
  requiredScope: string | null;
  parameters: OpenApiParameter[];
  requestBody: OpenApiRequestBody | null;
  responses: OpenApiResponse[];
  deprecated: boolean;
}

export interface OpenApiParameter {
  name: string;
  location: string;
  required: boolean;
  description: string;
  schema: string;
}

export interface OpenApiRequestBody {
  required: boolean;
  description: string;
  contentType: string;
  /** Human-readable field list, e.g. `track_id: string (required)`. */
  fields: string[];
}

export interface OpenApiResponse {
  status: string;
  description: string;
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

export function listApiKeys(): Promise<ApiKeyListResponse> {
  return apiRequest<ApiKeyListResponse>('/club/api-keys');
}

export function createApiKey(input: CreateApiKeyInput): Promise<CreatedApiKey> {
  return apiRequest<CreatedApiKey>('/club/api-keys', { method: 'POST', body: input });
}

export function getApiKey(id: string): Promise<ApiKeyView> {
  return apiRequest<ApiKeyView>(`/club/api-keys/${encodeURIComponent(id)}`);
}

export function patchApiKey(id: string, patch: PatchApiKeyInput): Promise<ApiKeyView> {
  return apiRequest<ApiKeyView>(`/club/api-keys/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    body: patch,
  });
}

/** New secret for the same key. `graceHours` keeps the old secret alive 0–72h. */
export function rotateApiKey(id: string, graceHours: number): Promise<RotatedApiKey> {
  return apiRequest<RotatedApiKey>(`/club/api-keys/${encodeURIComponent(id)}/rotate`, {
    method: 'POST',
    body: { grace_hours: graceHours },
  });
}

/** Immediate revocation (204). Effective on the next request the key makes. */
export function deleteApiKey(id: string): Promise<void> {
  return apiRequest<void>(`/club/api-keys/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

export function getApiKeyUsage(id: string): Promise<KeyUsage> {
  return apiRequest<KeyUsage>(`/club/api-keys/${encodeURIComponent(id)}/usage`);
}

// ---------------------------------------------------------------------------
// Documentation
// ---------------------------------------------------------------------------

export function getScopeCatalogue(): Promise<ScopeCatalogue> {
  return apiRequest<ScopeCatalogue>('/club/api-keys/scopes');
}

export function getDocsManifest(): Promise<DocsManifest> {
  return apiRequest<DocsManifest>('/club/developers/docs');
}

export function getDocsSection(id: string): Promise<DocsSection> {
  return apiRequest<DocsSection>(`/club/developers/docs/${encodeURIComponent(id)}`);
}

/** The whole OpenAPI 3.1 document. Shape is checked by `toOperations` below. */
export function getOpenApiDocument(): Promise<Record<string, unknown>> {
  return apiRequest<Record<string, unknown>>('/club/developers/openapi.json');
}

// ---------------------------------------------------------------------------
// OpenAPI → rows
// ---------------------------------------------------------------------------

const HTTP_METHODS = ['get', 'post', 'put', 'patch', 'delete', 'head', 'options'] as const;

type Json = Record<string, unknown>;

function asObject(value: unknown): Json | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Json : null;
}

function asString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

/** `{ type: 'string', format: 'uuid' }` → `string (uuid)`; `$ref` → the ref name. */
function describeSchema(schema: unknown, spec: Json): string {
  const node = asObject(schema);
  if (!node) return '';
  const ref = asString(node.$ref);
  if (ref) return ref.split('/').pop() ?? ref;
  const type = asString(node.type);
  const format = asString(node.format);
  const items = asObject(node.items);
  const base = type === 'array' && items
    ? `array<${describeSchema(items, spec)}>`
    : [type, format].filter(Boolean).join(' ');
  const enums = Array.isArray(node.enum) ? node.enum.filter((value) => typeof value === 'string') : [];
  return enums.length > 0 ? `${base || 'enum'} (${enums.join(' | ')})` : base;
}

/** `#/components/schemas/Request` → the resolved schema object, when present. */
function resolveRef(spec: Json, ref: string): Json | null {
  if (!ref.startsWith('#/')) return null;
  let node: unknown = spec;
  for (const part of ref.slice(2).split('/')) {
    const object = asObject(node);
    if (!object) return null;
    node = object[part];
  }
  return asObject(node);
}

/**
 * Request body as a short field list. A `$ref` is resolved so the panel shows
 * the actual fields instead of `$ref: #/components/schemas/...`.
 */
function describeRequestBody(spec: Json, body: unknown): OpenApiRequestBody | null {
  const node = asObject(body);
  if (!node) return null;
  const content = asObject(node.content);
  const first = content ? Object.entries(content)[0] : undefined;
  if (!first) return null;
  const [contentType, media] = first;
  let schema = asObject(asObject(media)?.schema);
  const ref = schema ? asString(schema.$ref) : '';
  if (ref) schema = resolveRef(spec, ref) ?? schema;
  const properties = schema ? asObject(schema.properties) : null;
  const required = Array.isArray(schema?.required)
    ? schema.required.filter((value): value is string => typeof value === 'string')
    : [];
  const fields = properties
    ? Object.entries(properties).map(([name, property]) => {
      const field = describeSchema(property, spec);
      const isRequired = required.includes(name);
      return `${name}: ${field || 'any'}${isRequired ? ' *' : ''}`;
    })
    : [];
  return {
    required: node.required === true,
    description: asString(node.description),
    contentType,
    fields,
  };
}

function describeParameters(spec: Json, raw: unknown): OpenApiParameter[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((entry) => {
    let node = asObject(entry);
    const ref = node ? asString(node.$ref) : '';
    if (ref) node = resolveRef(spec, ref) ?? node;
    if (!node) return [];
    const name = asString(node.name);
    if (!name) return [];
    return [{
      name,
      location: asString(node.in, 'query'),
      required: node.required === true,
      description: asString(node.description),
      schema: describeSchema(node.schema, spec),
    }];
  });
}

function describeResponses(raw: unknown): OpenApiResponse[] {
  const node = asObject(raw);
  if (!node) return [];
  return Object.entries(node).map(([status, value]) => {
    const response = asObject(value);
    return { status, description: asString(response?.description) };
  });
}

/**
 * Flattens the OpenAPI document into operation rows grouped by their first tag.
 *
 * Only the fields the reference view renders are read; a document with no
 * `paths` produces no rows at all, and the page reports that honestly instead
 * of showing an empty shell.
 */
export function toOpenApiOperations(spec: Record<string, unknown>): OpenApiOperation[] {
  const paths = asObject(spec.paths);
  if (!paths) return [];
  const operations: OpenApiOperation[] = [];
  for (const [path, rawItem] of Object.entries(paths)) {
    const item = asObject(rawItem);
    if (!item) continue;
    const sharedParameters = item.parameters;
    for (const method of HTTP_METHODS) {
      const operation = asObject(item[method]);
      if (!operation) continue;
      const scope = operation['x-required-scope'];
      const tags = Array.isArray(operation.tags)
        ? operation.tags.filter((tag): tag is string => typeof tag === 'string')
        : [];
      operations.push({
        method: method.toUpperCase(),
        path,
        summary: asString(operation.summary),
        description: asString(operation.description),
        operationId: asString(operation.operationId),
        tags,
        requiredScope: typeof scope === 'string' && scope.length > 0 ? scope : null,
        parameters: describeParameters(spec, operation.parameters ?? sharedParameters),
        requestBody: describeRequestBody(spec, operation.requestBody),
        responses: describeResponses(operation.responses),
        deprecated: operation.deprecated === true,
      });
    }
  }
  return operations;
}

/** Top-level tag order of the document, so sections keep the spec's order. */
export function toOpenApiTagOrder(spec: Record<string, unknown>): string[] {
  const tags = spec.tags;
  if (!Array.isArray(tags)) return [];
  return tags.flatMap((tag) => {
    const name = asString(asObject(tag)?.name);
    return name ? [name] : [];
  });
}

/**
 * A `curl` invocation for one operation, built from the operation itself.
 *
 * The whole point of the reference tab is that this line is copy-pasteable:
 * base URL from the docs manifest, path parameters left as `{name}` so the
 * caller sees what to substitute, and the key read from an environment
 * variable — never pasted into a file.
 */
export function toCurl(operation: OpenApiOperation, baseUrl: string): string {
  const base = baseUrl.replace(/\/$/, '');
  const head: string[] = [
    `curl -sS -X ${operation.method} \\`,
    `  '${base}${operation.path}' \\`,
    '  -H "Authorization: Bearer $DROPIT_API_KEY"',
  ];
  const hasBody = operation.method !== 'GET'
    && operation.method !== 'DELETE'
    && (operation.requestBody?.fields.length ?? 0) > 0;

  if (!hasBody) return head.join('\n');

  head[head.length - 1] = `  -H "Authorization: Bearer $DROPIT_API_KEY" \\`;
  head.push("  -H 'Content-Type: application/json' \\");

  const body: Record<string, string> = {};
  for (const field of operation.requestBody?.fields ?? []) {
    const name = field.split(':')[0]?.replace(' *', '').trim();
    if (name) body[name] = `<${name}>`;
  }
  head.push(`  -d '${JSON.stringify(body)}'`);
  return head.join('\n');
}
