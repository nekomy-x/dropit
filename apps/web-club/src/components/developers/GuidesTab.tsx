/**
 * Guides — the long-form documentation served by this deployment.
 *
 * Two columns at desktop widths: the manifest on the left (the reading order
 * the server declares), the selected section on the right. One column below
 * 960px, where the list becomes a scrolling strip above the body instead of a
 * second screen.
 *
 * Every state is real: the manifest and the section each have their own
 * loading, error and empty state, because a manifest can load while one section
 * fails, and saying so is better than blanking the whole tab.
 */
import { useCallback, useEffect, useState } from 'react';
import {
  getDocsManifest,
  getDocsSection,
  type DocsManifest,
  type DocsSection,
} from '../../api/developer';
import { useAsyncData } from '../../hooks/useAsyncData';
import { useLocale } from '../../locale';
import {
  EmptyState,
  ErrorState,
  Panel,
  Skeleton,
} from '../club';
import Markdown from './Markdown';

export default function GuidesTab({ target }: { target?: string }) {
  const { t } = useLocale();

  const fetchManifest = useCallback(() => getDocsManifest(), []);
  const manifest = useAsyncData<DocsManifest>(fetchManifest);

  const sections = manifest.data?.sections ?? [];
  const [selected, setSelected] = useState<string | null>(null);

  // The page can ask for a specific guide (the secret dialog links `quickstart`).
  // A target that the manifest does not list is ignored: the panel does not
  // invent a section id.
  useEffect(() => {
    if (target && sections.some((section) => section.id === target)) {
      setSelected(target);
      return;
    }
    if (sections.length > 0 && !sections.some((section) => section.id === selected)) {
      setSelected(sections[0]?.id ?? null);
    }
  }, [target, sections, selected]);

  const fetchSection = useCallback(
    () => (selected ? getDocsSection(selected) : Promise.resolve(null)),
    [selected],
  );
  const section = useAsyncData<DocsSection | null>(fetchSection);

  const isLoadingManifest = manifest.loading && manifest.data === null;

  return (
    <Panel
      title={t('developers.guidesPanel')}
      detail={t('developers.guidesDetail')}
      action={manifest.data ? (
        <span className="chip chip-neutral">{manifest.data.api_version}</span>
      ) : undefined}
    >
      {isLoadingManifest ? (
        <div className="flex flex-col gap-3" role="status" aria-label={t('developers.guidesLoading')}>
          <Skeleton height={28} />
          <Skeleton height={28} />
          <Skeleton height={28} width="70%" />
        </div>
      ) : null}

      {manifest.error ? (
        <ErrorState
          title={t('developers.guidesFailed')}
          message={manifest.error.message}
          code={manifest.error.code}
          onRetry={manifest.reload}
        />
      ) : null}

      {manifest.data && sections.length === 0 ? (
        <EmptyState
          title={t('developers.guidesEmptyTitle')}
          detail={t('developers.guidesEmptyDetail')}
        />
      ) : null}

      {manifest.data && sections.length > 0 ? (
        <div className="dev-guides">
          <nav className="dev-guides-nav" aria-label={t('developers.guidesPanel')}>
            <ul>
              {sections.map((entry) => {
                const active = entry.id === selected;
                return (
                  <li key={entry.id}>
                    <button
                      type="button"
                      className={`dev-guides-link ${active ? 'is-active' : ''}`.trim()}
                      aria-current={active ? 'true' : undefined}
                      onClick={() => setSelected(entry.id)}
                    >
                      <span className="dev-guides-link-title">{entry.title}</span>
                      <span className="dev-guides-link-summary">{entry.summary}</span>
                      <span className="dev-guides-link-scope">
                        {entry.requires.length > 0
                          ? t('developers.requiresLabel') + ': ' + entry.requires.join(', ')
                          : t('developers.requiresNone')}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </nav>

          <div className="dev-guides-body">
            {section.loading && !section.data ? (
              <div className="flex flex-col gap-3" role="status" aria-label={t('developers.sectionLoading')}>
                <Skeleton height={24} width="45%" />
                <Skeleton height={14} />
                <Skeleton height={14} />
                <Skeleton height={14} width="80%" />
                <Skeleton height={96} />
              </div>
            ) : null}

            {section.error && !section.error.notImplemented ? (
              <ErrorState
                title={t('developers.sectionFailed')}
                message={section.error.message}
                code={section.error.code}
                onRetry={section.reload}
              />
            ) : null}

            {section.error?.notImplemented ? (
              <EmptyState
                title={t('developers.guidesEmptyTitle')}
                detail={t('developers.guidesEmptyDetail')}
              />
            ) : null}

            {section.data ? (
              <>
                <h3 className="dev-guides-title">{section.data.title}</h3>
                <p className="dev-guides-summary">{section.data.summary}</p>
                <Markdown
                  markdown={section.data.markdown}
                  copyLabel={t('developers.copyCode')}
                  copiedLabel={t('developers.codeCopied')}
                  copyBlockedLabel={t('developers.copyBlocked')}
                  copyFailedLabel={t('developers.copyFailed')}
                />
              </>
            ) : null}
          </div>
        </div>
      ) : null}
    </Panel>
  );
}