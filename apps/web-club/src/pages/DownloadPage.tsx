import { useEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { QRCodeSVG } from 'qrcode.react';
import PublicBar from '../components/connect/PublicBar';
import Icon, { type IconName } from '../components/connect/Icons';
import BrandLogo from '../components/BrandLogo';
import { useLocale, type MessageKey } from '../connect/locale';
import { IS_DEMO } from '../app-mode';
import {
  GUEST_APK_PATH,
  GUEST_STORES,
  detectGuestPlatform,
  type GuestPlatform,
  type GuestStorePlatform,
} from '../connect/guest-app';
import './download.css';

/**
 * Guest-app download page (`/download`).
 *
 * Contract: docs/ui/hub-download-contract.md §Store links, §OS detection,
 * §Visual contract. The expected state in this repository is **Soon**: there is
 * no store listing and usually no published APK, so the page has to read as a
 * deliberate chooser rather than a broken download. It names the stores, states
 * the "coming soon" fact once, and offers the QR handoff as the real way to
 * reach the app today.
 *
 * A store button becomes a link only when its build-time URL is configured, and
 * the APK row appears only after a HEAD probe of the published file succeeds.
 */
const MOMENTS: { label: MessageKey; glyph: IconName }[] = [
  { label: 'dlScreenScan', glyph: 'scanQrCode' },
  { label: 'dlScreenPick', glyph: 'music2' },
  { label: 'dlScreenQueue', glyph: 'listMusic' },
];

const PLATFORM_LABEL: Record<GuestPlatform, MessageKey> = {
  ios: 'dlIos',
  android: 'dlAndroid',
  desktop: 'dlDesktop',
};

const PLATFORMS: GuestPlatform[] = ['ios', 'android', 'desktop'];

/** The two stores, in a stable order; the manual choice decides the emphasis. */
const STORE_ORDER: GuestStorePlatform[] = ['ios', 'android'];

/** One decimal, locale decimal mark (44.6 MB / 44,6 MB). */
function formatBytes(bytes: number, locale: string): string {
  const megabytes = bytes / (1024 * 1024);
  const mb = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 });
  if (megabytes >= 1) return `${mb.format(megabytes)} MB`;
  const kilobytes = bytes / 1024;
  if (kilobytes >= 1) {
    return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(kilobytes)} KB`;
  }
  return `${new Intl.NumberFormat(locale).format(bytes)} B`;
}

export default function DownloadPage() {
  const { t, locale } = useLocale();

  // Detection preselects once at mount; a manual choice is never re-detected.
  const [detected] = useState<GuestPlatform>(() => detectGuestPlatform());
  const [selected, setSelected] = useState<GuestPlatform>(detected);

  const [apk, setApk] = useState<{ size?: number } | null>(null);
  const [pageUrl] = useState(() =>
    typeof window === 'undefined' ? '' : `${window.location.origin}/download`,
  );

  const originalTitle = useRef<string | null>(null);

  useEffect(() => {
    if (originalTitle.current === null) originalTitle.current = document.title;
    document.title = t('dlTitle');
  }, [t]);

  useEffect(
    () => () => {
      if (originalTitle.current !== null) document.title = originalTitle.current;
    },
    [],
  );

  // The APK row is a test-stand capability, so it is probed rather than assumed.
  // A 404 (or an HTML SPA fallback) simply means "no row": this is the normal
  // state of a build without the file, and it must not log.
  useEffect(() => {
    let cancelled = false;
    const controller = new AbortController();
    fetch(GUEST_APK_PATH, { method: 'HEAD', signal: controller.signal })
      .then((response) => {
        if (cancelled || response.status !== 200) return;
        const contentType = response.headers.get('content-type')?.split(';')[0]?.trim().toLowerCase();
        const lengthHeader = response.headers.get('content-length');
        const length = Number(lengthHeader);
        const apkType = contentType === 'application/vnd.android.package-archive';
        // Static hosts such as Vite may omit the APK MIME type. A real file
        // length is required for that fallback; HTML SPA responses stay rejected.
        const binaryType = !contentType || contentType === 'application/octet-stream' || contentType === 'application/zip';
        if (!apkType && !binaryType) return;
        if (lengthHeader !== null && (!Number.isFinite(length) || length <= 0)) return;
        if (binaryType && lengthHeader === null) return;
        setApk(Number.isFinite(length) && length > 0 ? { size: length } : {});
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, []);

  const anyStoreLive = STORE_ORDER.some((id) => Boolean(GUEST_STORES[id].url));
  const selectedIsDesktop = selected === 'desktop';
  // One note, never two. "Soon" is the fact when no listing exists at all; on a
  // computer with a live listing the useful sentence is the phone handoff.
  const storeNote = !anyStoreLive
    ? t('dlSoonNote')
    : selectedIsDesktop
      ? t('dlDesktopNote')
      : null;

  return (
    <div className="connect-root download-root flex min-h-screen flex-col bg-drop-bg text-drop-text">
      <PublicBar active="guests" />

      <main className="dl-main flex-1">
        <div className="dl-shell">
          <div className="dl-grid">
            <div className="dl-copy">
              <Link to={IS_DEMO ? '/hub' : '/'} className="dl-back">
                <span className="dl-back-icon" aria-hidden="true">
                  <Icon name="arrowRight" size={13} strokeWidth={2} />
                </span>
                {t('dlBackHub')}
              </Link>

              <p className="connect-eyebrow mt-3">{t('dlEyebrow')}</p>
              <h1 className="connect-display dl-title mt-3.5">{t('dlTitle')}</h1>
              <p className="dl-lead mt-4">{t('dlCopy')}</p>
              <p className="dl-note mt-2.5">{t('dlNote')}</p>

              {/* Below 1024px the app preview is hidden, so the three moments
                  are summarised here. From 1024px the preview carries the same
                  three labels and this row steps aside: exactly one instance of
                  each label at any width. */}
              <ol className="dl-steps mt-7">
                {MOMENTS.map((moment, index) => (
                  <li className="dl-step" key={moment.label}>
                    <span className="dl-step-num" aria-hidden="true">
                      {String(index + 1).padStart(2, '0')}
                    </span>
                    <span className="dl-step-label">{t(moment.label)}</span>
                  </li>
                ))}
              </ol>

              <section className="dl-stores dl-rule mt-8" aria-labelledby="dl-manual-label">
                <div className="dl-stores-head">
                  <p className="connect-mono-label" id="dl-manual-label">
                    {t('dlManual')}
                  </p>
                  <div className="dl-seg" role="group" aria-labelledby="dl-manual-label">
                    {PLATFORMS.map((platform) => (
                      <button
                        key={platform}
                        type="button"
                        className="dl-seg-btn"
                        aria-pressed={selected === platform}
                        onClick={() => setSelected(platform)}
                      >
                        {t(PLATFORM_LABEL[platform])}
                      </button>
                    ))}
                  </div>
                </div>

                <p className="dl-detected mt-2.5">
                  {t('dlDetected', { os: t(PLATFORM_LABEL[detected]) })}
                </p>

                {anyStoreLive ? null : <h2 className="dl-unavailable mt-5">{t('dlUnavailable')}</h2>}

                {apk && selected !== 'ios' ? (
                  <a
                    className={`dl-apk mt-5${selected === 'android' && !GUEST_STORES.android.url ? ' is-primary' : ''}`}
                    href={GUEST_APK_PATH}
                    download
                  >
                    <span className="dl-apk-icon" aria-hidden="true">
                      <Icon name="doc" size={16} />
                    </span>
                    <span className="dl-apk-copy">
                      <span className="dl-apk-title">{t('dlApk')}</span>
                      <span className="dl-apk-note">{t('dlApkNote')}</span>
                    </span>
                    {apk.size ? <span className="dl-apk-size">{formatBytes(apk.size, locale)}</span> : null}
                    <Icon name="arrowUpRight" size={14} strokeWidth={2} />
                  </a>
                ) : null}

                <div className="dl-store-actions">
                  {STORE_ORDER.map((id) => {
                    const store = GUEST_STORES[id];
                    const primary = selected === id && !(id === 'android' && apk && !store.url);
                    const className = `dl-store-btn ${primary ? 'is-primary' : 'is-quiet'}`;

                    // Live: the button is the listing itself; the click also
                    // moves the emphasis so the pair always agrees with the
                    // segmented control.
                    if (store.url) {
                      return (
                        <a
                          key={id}
                          className={className}
                          href={store.url}
                          target="_blank"
                          rel="noreferrer"
                          onClick={() => setSelected(id)}
                        >
                          <span className="dl-store-name">
                            {t('dlOpenStore', { store: store.name })}
                          </span>
                          <Icon name="arrowUpRight" size={14} strokeWidth={2} />
                        </a>
                      );
                    }

                    // Soon: still a real button, never a dead end — it selects
                    // the platform so the primary action follows the visitor.
                    return (
                      <button
                        key={id}
                        type="button"
                        className={className}
                        onClick={() => setSelected(id)}
                        aria-pressed={selected === id}
                      >
                        <span className="dl-store-name">{store.name}</span>
                        <span className="dl-soon-chip">{t('dlSoon')}</span>
                      </button>
                    );
                  })}
                </div>

                {storeNote ? <p className="dl-store-note mt-3">{storeNote}</p> : null}
              </section>

              {/* Desktop has no store: the QR is the handoff to the phone. */}
              {selectedIsDesktop ? (
                <div className="dl-qr dl-rule mt-8">
                  <div className="dl-qr-tile">
                    {pageUrl ? (
                      <QRCodeSVG
                        value={pageUrl}
                        size={148}
                        level="M"
                        marginSize={0}
                        bgColor="#FFFFFF"
                        fgColor="#0A0C0B"
                        title={t('dlQrTitle')}
                      />
                    ) : null}
                  </div>
                  <div className="dl-qr-copy">
                    <p className="dl-qr-title">{t('dlQrTitle')}</p>
                    <p className="dl-qr-note mt-1.5">{t('dlQrNote')}</p>
                  </div>
                </div>
              ) : null}

              <div className="dl-meta dl-rule mt-8">
                <p className="dl-price-note">{t('dlPriceNote')}</p>
                <a className="dl-support" href="mailto:hello@dropit.music">
                  <Icon name="mail" size={13} />
                  {t('dlSupport')}
                </a>
              </div>
            </div>

            {/* App preview: HTML/CSS from the drop tokens plus the two artwork
                thumbnails that already ship. A static picture of the app's
                three moments — no live data, no invented counters. */}
            <div className="dl-preview">
              <div className="dl-device" role="group" aria-label={t('dlEyebrow')}>
                <div className="dl-device-screen">
                  <span className="dl-device-notch" aria-hidden="true" />
                  <div className="dl-device-head">
                    <BrandLogo size="tiny" />
                  </div>
                  <div className="dl-device-body">
                    {MOMENTS.map((moment, index) => (
                      <div className="dl-moment" key={moment.label}>
                        <span className="dl-moment-num" aria-hidden="true">
                          {String(index + 1).padStart(2, '0')}
                        </span>
                        <span className="dl-moment-label">{t(moment.label)}</span>
                        {index === 1 ? (
                          <span className="dl-moment-stack" aria-hidden="true">
                            <img src="/art-one-more-time.jpg" alt="" loading="lazy" />
                            <img src="/art-move.jpg" alt="" loading="lazy" />
                          </span>
                        ) : (
                          <span className="dl-moment-glyph" aria-hidden="true">
                            <Icon name={moment.glyph} size={15} />
                          </span>
                        )}
                      </div>
                    ))}
                  </div>
                  <div className="dl-device-tabs" aria-hidden="true">
                    <span><Icon name="headphones" size={15} /></span>
                    <span><Icon name="compass" size={15} /></span>
                    <span><Icon name="listMusic" size={15} /></span>
                    <span><Icon name="userRound" size={15} /></span>
                  </div>
                  <span className="dl-device-home" aria-hidden="true" />
                </div>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}
