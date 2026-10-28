import { Link } from 'react-router-dom';
import { LogoLockup } from './ConnectNav';
import { useLocale } from '../../connect/locale';

export default function ConnectFooter() {
  const { t } = useLocale();
  const columns = [
    { title: t('product'), links: [['features', '/connect#benefits'], ['revenue', '/connect#revenue'], ['steps', '/connect#steps'], ['guest', '/connect#guest']] },
    { title: t('venues'), links: [['panel', '/login'], ['djApp', '/connect#panel'], ['qrMaterials', '/connect/qr'], ['support', 'mailto:clubs@dropit.music']] },
    { title: t('company'), links: [['about', '/connect#hero'], ['partners', 'mailto:clubs@dropit.music'], ['press', 'mailto:press@dropit.music'], ['contact', 'mailto:clubs@dropit.music']] },
  ] as const;
  return (
    <footer className="border-t border-drop-line bg-drop-bg">
      <div className="mx-auto w-full max-w-[1440px] px-6 py-14 lg:px-12">
        <div className="grid gap-10 lg:grid-cols-[1.4fr_repeat(3,minmax(0,1fr))]">
          <div className="max-w-[320px]">
            <LogoLockup compact />
            <p className="mt-4 text-[13px] leading-relaxed text-drop-muted">
              {t('footerBlurb')}
            </p>
          </div>

          {columns.map((col) => (
            <div key={col.title}>
              <h3 className="connect-mono-label">{col.title}</h3>
              <ul className="mt-4 space-y-2.5">
                {col.links.map(([labelKey, to], index) => (
                  <li key={`${col.title}-${labelKey}-${index}`}>
                    {to.startsWith('mailto:') ? (
                      <a
                        href={to}
                        className="text-[13px] text-drop-muted transition-colors hover:text-drop-accent"
                      >
                        {t(labelKey)}
                      </a>
                    ) : (
                      <Link
                        to={to}
                        className="text-[13px] text-drop-muted transition-colors hover:text-drop-accent"
                      >
                        {t(labelKey)}
                      </Link>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        <div className="mt-12 flex flex-col gap-4 border-t border-drop-line pt-6 sm:flex-row sm:items-center sm:justify-between">
          <p className="font-mono text-[10px] uppercase tracking-[1.2px] text-drop-dim">
            {t('copyright')}
          </p>
          <div className="flex flex-wrap items-center gap-5">
            {['Telegram', 'Instagram', 'SoundCloud'].map((s) => (
              <a
                key={s}
                href="https://dropit.music"
                target="_blank"
                rel="noreferrer"
                className="font-mono text-[10px] uppercase tracking-[1.2px] text-drop-muted transition-colors hover:text-drop-accent"
              >
                {s}
              </a>
            ))}
            <a
              href="https://dropit.music"
              target="_blank"
              rel="noreferrer"
              className="font-mono text-[10px] uppercase tracking-[1.2px] text-drop-dim transition-colors hover:text-drop-muted"
            >
              {t('privacy')}
            </a>
          </div>
        </div>
      </div>
    </footer>
  );
}
