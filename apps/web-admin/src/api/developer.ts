/**
 * Admin API-key oversight — read-only, plus revocation.
 *
 * Contract: ADR-010 §5 ("Admin sees all keys read-only and can revoke any
 * key") and the admin panel part of `docs/openapi.yaml`:
 *
 *   GET  /v1/admin/api-keys            → { items: AdminApiKeyView[] }
 *   POST /v1/admin/api-keys/{id}/revoke→ 204
 *   GET  /v1/admin/api-keys/{id}/usage → { summary, items }
 *   GET  /v1/admin/api-keys/scopes     → scope catalogue
 *
 * Transport and auth are the admin client's: every call goes through
 * `apiRequest`, so the in-memory Bearer token and the single 401 refresh stay
 * in `api/client.ts`. There is no secret field here by construction — the
 * plaintext exists only in the club's create/rotate response.
 */
import { apiRequest } from './client';

export type ApiKeyEnvironment = 'live' | 'test';
export type ApiKeyStatus = 'active' | 'revoked';

/** One key of any club. `club_name` is the join the admin list adds. */
export interface AdminApiKeyView {
  id: string;
  club_id: string;
  club_name: string;
  name: string;
  prefix: string;
  environment: ApiKeyEnvironment;
  scopes: string[];
  status: ApiKeyStatus;
  rate_limit_per_min: number;
  allowed_cidrs: string[];
  expires_at: string | null;
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
  rotated_from: string | null;
  expired: boolean;
}

export interface AdminApiKeyListResponse {
  items: AdminApiKeyView[];
}

export interface ScopeInfo {
  id: string;
  title: string;
  description: string;
  group: string;
  risk: string;
  docs_section: string;
}

export interface ScopeCatalogue {
  scopes: ScopeInfo[];
  presets: Array<{ id: string; title: string; description: string; scopes: string[] }>;
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

/**
 * `limit` is the API's page size. The panel asks for the maximum it accepts so
 * the operator sees one list; a deployment with more keys than that shows the
 * count it actually received and links nothing it did not read.
 */
export function listAdminApiKeys(limit = 200): Promise<AdminApiKeyListResponse> {
  return apiRequest<AdminApiKeyListResponse>(`/admin/api-keys?limit=${encodeURIComponent(String(limit))}`);
}

/** Immediate revocation of any key, whichever club owns it (204). */
export function revokeAdminApiKey(id: string): Promise<void> {
  return apiRequest<void>(`/admin/api-keys/${encodeURIComponent(id)}/revoke`, { method: 'POST' });
}

export function getAdminApiKeyUsage(id: string): Promise<KeyUsage> {
  return apiRequest<KeyUsage>(`/admin/api-keys/${encodeURIComponent(id)}/usage`);
}

export function getAdminScopeCatalogue(): Promise<ScopeCatalogue> {
  return apiRequest<ScopeCatalogue>('/admin/api-keys/scopes');
}