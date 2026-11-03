/**
 * Reference — the public API v1, read from the deployment's own OpenAPI document.
 *
 * The table is generated, never written by hand: `toOpenApiOperations` flattens
 * `paths` into rows and `toCurl` builds the copy-pasteable call from the same
 * row. That is what keeps the panel honest — an operation the binary does not
 * serve cannot appear here, because it does not appear in the document.
 *
 * Layout: one section per first tag, operations in document order. Details are
 * a disclosure, closed by default, so the list stays scannable and the page does
 * not open 19 paragraphs at once.
 */
import { useCallback, useMemo, useState } from 'react';
import {
  getDocsManifest,
  getOpenApiDocument,
  toCurl,
  toOpenApiOperations,
  toOpenApiTagOrder,
  type DocsManifest,
  type OpenApiOperation,
} from '../../api/developer';
import { useAsyncData } from '../../hooks/useAsyncData';
import { useLocale } from '../../locale';
import {
  EmptyState,
  ErrorState,
  Panel,
  Skeleton,
} from '../club';
import Icon from '../Icon';

const METHOD_TONE: Record<string, string> = {
  GET: 'chip-accent',
  POST: 'chip-warn',
  PATCH: 'chip-warn',
  PUT: 'chip-warn',
  DELETE: 'chip-error',
};

function methodClass(method: string): string {
  return `chip dev-method ${METHOD_TONE[method] ?? 'chip-neutral'}`;
}

export default function ReferenceTab() {
  const { t } = useLocale();

  const fetchSpec = useCallback(async () => {
    // The manifest carries `base_url` (the path a key calls), and the document
    // carries the operations. They are fetched together because a curl line
    // needs both and neither is useful alone.
    const [spec, manifest] = await Promise.all([getOpenApiDocument(), getDocsManifest()]);
    return { spec, manifest };
  }, []);

  const { data, loading, error, reload } = useAsyncData<{ spec: Record<string, unknown>; manifest: DocsManifest }>(fetchSpec);
  const [copied, setCopied] = useState<string | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);

  const operations = useMemo(() => (data ? toOpenApiOperations(data.spec) : []), [data]);
  const tagOrder = useMemo(() => (data ? toOpenApiTagOrder(data.spec) : []), [data]);

  /** Sections in the document's own tag order, then any tag it did not declare. */
  const sections = useMemo(() => {
    const byTag = new Map<string, OpenApiOperation[]>();
    for (const operation of operations) {
      const tag = operation.tags[0] ?? '';
      const list = byTag.get(tag) ?? [];
      list.push(operation);
      byTag.set(tag, list);
    }
    const ordered = [...byTag.keys()].sort((a, b) => {
      const ai = tagOrder.indexOf(a);
      const bi = tagOrder.indexOf(b);
      if (ai === -1 && bi === -1) return a.localeCompare(b);
      if (ai === -1) return 1;
      if (bi === -1) return -1;
      return ai - bi;
    });
    return ordered.map((tag) => ({ tag, items: byTag.get(tag) ?? [] }));
  }, [operations, tagOrder]);

  const copyCurl = async (operation: OpenApiOperation) => {
    if (!data) return;
    const text = toCurl(operation, data.manifest.base_url);
    if (!navigator.clipboard) {
      setCopyError(t('developers.copyBlocked'));
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      setCopyError(null);
      setCopied(`${operation.method} ${operation.path}`);
      window.setTimeout(() => setCopied(null), 4000);
    } catch {
      setCopyError(t('developers.copyFailed'));
    }
  };

  return (
    <Panel
      title={t('developers.referencePanel')}
      detail={t('developers.referenceDetail')}
      action={data ? (
        <span className="chip chip-neutral">{t('developers.referenceCount', { count: operations.length })}</span>
      ) : undefined}
    >
      {loading && !data ? (
        <div className="flex flex-col gap-3" role="status" aria-label={t('developers.referenceLoading')}>
          <Skeleton height={40} />
          <Skeleton height={40} />
          <Skeleton height={40} width="85%" />
        </div>
      ) : null}

      {error ? (
        <ErrorState
          title={t('developers.referenceFailed')}
          message={error.message}
          code={error.code}
          onRetry={reload}
        />
      ) : null}

      {data && data.manifest.api_version ? (
        <p className="dev-ref-meta">
          <span>{t('developers.apiVersion', { version: data.manifest.api_version })}</span>
          <span className="dev-ref-meta-sep" aria-hidden="true">·</span>
          <span>{t('developers.baseUrl')}: <code className="dev-md-code-inline">{data.manifest.base_url}</code></span>
        </p>
      ) : null}

      {data && operations.length === 0 ? (
        <EmptyState
          title={t('developers.referenceEmptyTitle')}
          detail={t('developers.referenceEmptyDetail')}
        />
      ) : null}

      {copyError ? <p className="dev-ref-copy-error" role="alert">{copyError}</p> : null}

      {sections.map((section) => (
        <section key={section.tag || 'untagged'} className="dev-ref-section" aria-label={section.tag || t('developers.referencePanel')}>
          <h3 className="dev-ref-section-title">{section.tag || t('developers.referencePanel')}</h3>
          <ul className="dev-ref-list">
            {section.items.map((operation) => {
              const id = `${operation.method}-${operation.path}`;
              return (
                <li key={id}>
                  <details className="dev-op">
                    <summary className="dev-op-summary">
                      <span className={methodClass(operation.method)}>{operation.method}</span>
                      <code className="dev-op-path">{operation.path}</code>
                      <span className="dev-op-title">{operation.summary || operation.operationId}</span>
                      <span className="dev-op-scope chip chip-neutral">
                        {operation.requiredScope
                          ? t('developers.requiredScope', { scope: operation.requiredScope })
                          : t('developers.noScope')}
                      </span>
                      {operation.deprecated ? (
                        <span className="chip chip-warn">{t('developers.deprecated')}</span>
                      ) : null}
                    </summary>

                    <div className="dev-op-body">
                      {operation.description ? (
                        <p className="dev-op-description">{operation.description}</p>
                      ) : null}

                      <div className="dev-op-columns">
                        <div>
                          <p className="dev-op-label">{t('developers.parameters')}</p>
                          {operation.parameters.length === 0 ? (
                            <p className="dev-op-empty">{t('developers.parametersEmpty')}</p>
                          ) : (
                            <ul className="dev-op-params">
                              {operation.parameters.map((parameter) => (
                                <li key={`${parameter.location}-${parameter.name}`}>
                                  <code className="dev-md-code-inline">{parameter.name}</code>
                                  <span className="dev-op-param-loc">{parameter.location}</span>
                                  {parameter.schema ? <span className="dev-op-param-type">{parameter.schema}</span> : null}
                                  {parameter.required ? (
                                    <span className="dev-op-required">{t('developers.parameterRequired')}</span>
                                  ) : null}
                                </li>
                              ))}
                            </ul>
                          )}
                        </div>

                        <div>
                          <p className="dev-op-label">
                            {t('developers.requestBody')}
                            {operation.requestBody?.required ? ` · ${t('developers.requestBodyRequired')}` : ''}
                          </p>
                          {!operation.requestBody || operation.requestBody.fields.length === 0 ? (
                            <p className="dev-op-empty">{t('developers.requestBodyEmpty')}</p>
                          ) : (
                            <ul className="dev-op-params">
                              {operation.requestBody.fields.map((field) => (
                                <li key={field}><code className="dev-md-code-inline">{field}</code></li>
                              ))}
                            </ul>
                          )}
                        </div>
                      </div>

                      <div>
                        <p className="dev-op-label">{t('developers.responses')}</p>
                        <ul className="dev-op-responses">
                          {operation.responses.map((response) => (
                            <li key={response.status}>
                              <span className="dev-op-status">{response.status}</span>
                              <span>{response.description}</span>
                            </li>
                          ))}
                        </ul>
                      </div>

                      <div className="dev-op-foot">
                        <button
                          type="button"
                          className="btn btn-secondary btn-sm"
                          aria-label={t('developers.copyCurlFor', { method: operation.method, path: operation.path })}
                          onClick={() => { void copyCurl(operation); }}
                        >
                          <Icon name="lucide:copy" size={14} />
                          {copied === id ? t('developers.copied') : t('developers.copyCurl')}
                        </button>
                      </div>
                    </div>
                  </details>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </Panel>
  );
}