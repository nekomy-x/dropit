import type { HealthIndicator, HealthStatus } from './types';
import { useLocale } from '../../locale';

export interface HealthIndicatorsProps {
  services: ReadonlyArray<HealthIndicator>;
  title?: string;
  className?: string;
  /** Where the readings come from; the caller must keep this honest. */
  caption?: string;
}

const STATUS_CHIP: Record<HealthStatus, { key: string; tone: string }> = {
  healthy: { key: 'status.healthy', tone: 'chip-accent' },
  degraded: { key: 'status.degraded', tone: 'chip-warn' },
  outage: { key: 'status.outage', tone: 'chip-error' },
  unknown: { key: 'status.unknown', tone: 'chip-neutral' },
};

/**
 * Service health as a `list-row` panel: name and detail on the left, reading on
 * the right. The state is a chip with a word, so it never relies on colour.
 */
export default function HealthIndicators({ services, title, className = '', caption }: HealthIndicatorsProps) {
  const { t } = useLocale();
  return (
    <section className={`panel-card ${className}`.trim()} aria-labelledby="analytics-health-heading">
      <header className="panel-card-head">
        <div>
          <h2 id="analytics-health-heading" className="panel-card-title">{title ?? t('analytics.systemHealth')}</h2>
          <p className="panel-card-sub">{caption ?? t('analytics.liveChecks')}</p>
        </div>
      </header>

      {services.length === 0 ? (
        <div className="panel-empty">
          <strong>{t('analytics.noHealth')}</strong>
          <span>{t('analytics.noHealthBody')}</span>
        </div>
      ) : (
        <ul className="list" style={{ margin: 0, padding: 0, listStyle: 'none' }}>
          {services.map((service) => {
            const status = STATUS_CHIP[service.status];
            return (
              <li key={service.name} className="list-row">
                <span className="list-main">
                  <span className="list-title">{service.name}</span>
                  <span className="list-sub">{service.detail}</span>
                </span>
                {service.latency ? (
                  <span className="list-metric" style={{ fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-12)' }}>{service.latency}</span>
                ) : null}
                <span className={`chip ${status.tone}`}>
                  {t(status.key)}
                </span>
                {service.updatedAt ? (
                  <span className="list-sub" style={{ fontFamily: 'var(--font-mono)', flex: '0 0 auto' }}>{service.updatedAt}</span>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
