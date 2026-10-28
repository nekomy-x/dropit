import Icon from '../Icons';
import BrandLogo from '../../BrandLogo';
import { useLocale } from '../../../connect/locale';

/**
 * Guest-app mockup that floats beside the hero copy.
 *
 * Geometry is transcribed 1:1 from frame h7akdu / Hero / Phone mockup:
 * a 460×600 stage holding a 262×548 device (9px bezel, 30px screen radius)
 * with four floating chips. Everything is absolutely positioned on the
 * design's own pixel grid, so the mockup matches the frame without scaling.
 * Purely decorative — the interactive guest flow lives in the mobile app.
 */

const TABS = [
  { key: 'phoneListen', icon: 'headphones', color: '#00FF87', labelColor: '#F0F4F1', weight: 600 },
  { key: 'phoneExplore', icon: 'compass', color: '#929A95', labelColor: '#929A95', weight: 500 },
  { key: 'phoneRequests', icon: 'listMusic', color: '#919994', labelColor: '#919994', weight: 500 },
  { key: 'phoneProfile', icon: 'userRound', color: '#929A95', labelColor: '#929A95', weight: 500 },
] as const;

const QUEUE = [
  ['Techno Soul', '· ANNA'],
  ['Midnight City', '· M83'],
  ['Respect', '· A. Franklin'],
];

const TRACKS = [
  { title: 'Move', meta: 'Adam Port, Stryv · Afro house', art: '/art-move.jpg' },
  { title: 'One More Time', meta: 'Daft Punk · Dance classics', art: '/art-one-more-time.jpg' },
];

/** Shared chrome for the three floating chips (frame: $drop-surface + $drop-border). */
const chip: React.CSSProperties = {
  position: 'absolute',
  background: '#191919',
  border: '1px solid #FFFFFF17',
  boxShadow: '0 10px 24px #00000080',
};

export default function PhoneMockup() {
  const { t } = useLocale();
  return (
    <div className="relative h-[600px] w-[460px]">
      {/* Under glow + ground shadow beneath the levitating device. */}
      <div
        aria-hidden="true"
        style={{
          position: 'absolute',
          left: 100,
          top: 512,
          width: 260,
          height: 70,
          background: 'radial-gradient(closest-side, #FF8A5C1F, #FF8A5C00)',
        }}
      />
      <div
        aria-hidden="true"
        style={{
          position: 'absolute',
          left: 135,
          top: 560,
          width: 190,
          height: 28,
          borderRadius: '50%',
          background: '#000000B3',
          filter: 'blur(16px)',
        }}
      />

      {/* Side buttons, flush against the device shell. */}
      <div
        aria-hidden="true"
        style={{ position: 'absolute', left: 364, top: 168, width: 3, height: 64, borderRadius: 2, background: '#B5B3AD' }}
      />
      <div
        aria-hidden="true"
        style={{ position: 'absolute', left: 99, top: 178, width: 3, height: 44, borderRadius: 2, background: '#B5B3AD' }}
      />
      <div
        aria-hidden="true"
        style={{ position: 'absolute', left: 99, top: 230, width: 3, height: 44, borderRadius: 2, background: '#B5B3AD' }}
      />

      {/* Device shell — photo of a real phone body, screen inset by 9px. */}
      <div
        style={{
          position: 'absolute',
          left: 102,
          top: 9,
          width: 262,
          height: 548,
          borderRadius: 35,
          padding: 9,
          backgroundImage: 'url(/phone-body.png)',
          backgroundSize: 'cover',
          backgroundPosition: 'center',
          boxShadow: 'inset 0 0 0 2px #242424, 0 32px 64px #000000B3, 0 4px 16px #00000066',
        }}
      >
        <div
          style={{
            position: 'relative',
            width: '100%',
            height: '100%',
            borderRadius: 30,
            background: '#0A0C0B',
            overflow: 'hidden',
          }}
        >
          {/* Home screen */}
          <div
            style={{
              position: 'absolute',
              left: 5,
              top: 12,
              width: 234,
              height: 506,
              borderRadius: 14,
              background: '#121212',
              overflow: 'hidden',
            }}
          >
            {/* Stage photo header + contrast scrim */}
            <div
              style={{
                position: 'absolute',
                left: 7.2,
                top: 46.8,
                width: 219.6,
                height: 110.4,
                borderRadius: 12,
                backgroundImage: 'url(/phone-hero.jpg)',
                backgroundSize: 'cover',
                backgroundPosition: 'center',
              }}
            />
            <div
              style={{
                position: 'absolute',
                left: 7.2,
                top: 46.8,
                width: 219.6,
                height: 110.4,
                borderRadius: 12,
                background: 'linear-gradient(180deg, #090909D9 0%, #09090905 100%)',
              }}
            />

            {/* Nearby map tile + mint QR tile */}
            <div
              style={{
                position: 'absolute',
                left: 7.2,
                top: 166.8,
                width: 109.8,
                height: 84,
                borderRadius: '13.2px 0 0 13.2px',
                overflow: 'hidden',
                backgroundImage: 'url(/nearby-map.png)',
                backgroundSize: 'cover',
                backgroundPosition: 'center',
              }}
            >
              <div
                style={{
                  position: 'absolute',
                  left: 0,
                  top: 39.6,
                  width: 109.8,
                  height: 42,
                  background: 'linear-gradient(180deg, #10101600 0%, #101016E6 100%)',
                }}
              />
              <span
                style={{
                  position: 'absolute',
                  left: 10.8,
                  top: 45,
                  fontSize: 9.6,
                  lineHeight: 1.25,
                  fontWeight: 600,
                  color: '#FFFFFF',
                }}
              >
                {t('phoneNearby')}
              </span>
              <span
                style={{
                  position: 'absolute',
                  left: 10.8,
                  top: 65.4,
                  fontSize: 6,
                  color: '#FFFFFF',
                }}
              >
                {t('phoneDistance')}
              </span>
            </div>

            <div
              style={{
                position: 'absolute',
                left: 117,
                top: 166.8,
                width: 109.8,
                height: 81.6,
                borderRadius: '0 13.2px 13.2px 0',
                background: '#00FF87',
              }}
            >
              <span
                style={{
                  position: 'absolute',
                  left: 10.8,
                  top: 45,
                  fontSize: 9.6,
                  lineHeight: 1.25,
                  fontWeight: 600,
                  color: '#101813',
                }}
              >
                {t('phoneScan')}
              </span>
              <span
                style={{
                  position: 'absolute',
                  left: 10.8,
                  top: 65.4,
                  fontSize: 6,
                  color: '#17452F',
                }}
              >
                {t('phoneInstant')}
              </span>
              <div
                style={{
                  position: 'absolute',
                  left: 71.4,
                  top: 9.6,
                  width: 28.8,
                  height: 28.8,
                  borderRadius: 9.6,
                  background: '#101813',
                }}
              >
                  <span style={{ position: 'absolute', left: 6, top: 6, color: '#00FF87' }}>
                  <Icon name="scanQrCode" size={16.8} strokeWidth={2} />
                </span>
              </div>
            </div>

            {/* Section label + nearest venue row */}
            <span
              style={{
                position: 'absolute',
                left: 12,
                top: 262.8,
                fontSize: 6.6,
                fontWeight: 600,
                letterSpacing: 0.6,
                color: '#B8C0BA',
              }}
            >
              {t('phoneNight')}
            </span>
            <div
              style={{
                position: 'absolute',
                left: 12,
                top: 278.4,
                width: 210,
                height: 43.2,
                display: 'flex',
                alignItems: 'center',
                gap: 7.2,
                padding: '7.2px 0',
                background: '#121212',
                borderBottom: '0.6px solid #FFFFFF14',
                overflow: 'hidden',
              }}
            >
              <div
                style={{
                  width: 24,
                  height: 24,
                  borderRadius: 3.6,
                  flexShrink: 0,
                  backgroundImage: 'url(/venue-katerblau.jpg)',
                  backgroundSize: 'cover',
                  backgroundPosition: 'center',
                }}
              />
              <div style={{ display: 'flex', flexDirection: 'column', gap: 3, flex: 1, minWidth: 0 }}>
                <span style={{ fontSize: 8.4, lineHeight: 1, fontWeight: 500, color: '#E5E7E6' }}>
                  Kater Blau
                </span>
                <span style={{ fontSize: 7.2, lineHeight: 1, color: '#9A9F9C' }}>
                  {t('phoneAway')}
                </span>
              </div>
              <span style={{ fontSize: 8.4, lineHeight: 1, fontWeight: 600, color: '#00FF87' }}>
                {t('phoneJoin')}
              </span>
              <span style={{ color: '#9A9F9C', display: 'flex' }}>
                <Icon name="chevronRight" size={10.8} strokeWidth={2} />
              </span>
            </div>

            {/* Brand + city */}
            <div
              style={{
                position: 'absolute',
                left: 12,
                top: 14.4,
                width: 210,
                height: 21.6,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: 6,
              }}
            >
              <BrandLogo size="tiny" />
              <div style={{ display: 'flex', alignItems: 'center', gap: 3.6, padding: '6px 0' }}>
                <span style={{ color: '#00FF87', display: 'flex' }}>
                  <Icon name="pin" size={9} strokeWidth={2} />
                </span>
                <span style={{ fontSize: 7.8, lineHeight: 1, color: '#F2F5F3' }}>Berlin</span>
                <span style={{ color: '#969F99', display: 'flex' }}>
                  <Icon name="chevronDown" size={8.4} strokeWidth={2.4} />
                </span>
              </div>
            </div>

            {/* Tonight badge + headline over the photo */}
            <div
              style={{
                position: 'absolute',
                left: 16.8,
                top: 56.4,
                display: 'flex',
                alignItems: 'center',
                gap: 3.6,
              }}
            >
              <span
                style={{
                  fontSize: 5.4,
                  lineHeight: 1,
                  fontWeight: 700,
                  letterSpacing: 0.6,
                  color: '#FFFFFF',
                }}
              >
                {t('phoneLiveTonight')}
              </span>
            </div>
            <div
              style={{
                position: 'absolute',
                left: 16.8,
                top: 73.8,
                width: 186,
                display: 'flex',
                flexDirection: 'column',
                gap: 6,
              }}
            >
              <span
                className="font-display"
                style={{
                  fontSize: 20.4,
                  lineHeight: 1.08,
                  whiteSpace: 'pre-line',
                  color: '#F2F5F3',
                }}
              >
                {t('phoneHeroTitle')}
              </span>
              <span style={{ fontSize: 6.6, lineHeight: 1.2, color: '#DEE5E1' }}>
                {t('phoneHeroCopy')}
              </span>
            </div>

            {/* Recommendations */}
            <div
              style={{
                position: 'absolute',
                left: 12,
                top: 339.6,
                width: 210,
                display: 'flex',
                flexDirection: 'column',
                gap: 7.2,
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <span style={{ fontSize: 10.8, fontWeight: 700, color: '#F2F5F3' }}>{t('phoneMood')}</span>
                <div style={{ display: 'flex', alignItems: 'center', gap: 2.4 }}>
                  <span style={{ fontSize: 6.6, fontWeight: 600, color: '#00FF87' }}>{t('phoneBrowse')}</span>
                  <span style={{ color: '#00FF87', display: 'flex' }}>
                    <Icon name="arrowUpRight" size={8.4} strokeWidth={2.4} />
                  </span>
                </div>
              </div>

              {TRACKS.map((track) => (
                <div key={track.title} style={{ display: 'flex', alignItems: 'center', gap: 7.2 }}>
                  <div
                    style={{
                      width: 30,
                      height: 30,
                      borderRadius: 4.8,
                      flexShrink: 0,
                      backgroundImage: `url(${track.art})`,
                      backgroundSize: 'cover',
                      backgroundPosition: 'center',
                    }}
                  />
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 3, flex: 1, minWidth: 0, height: 22 }}>
                    <span style={{ fontSize: 8.4, fontWeight: 600, color: '#F2F5F3' }}>{track.title}</span>
                    <span style={{ fontSize: 6, color: '#B8C0BA' }}>{track.meta}</span>
                  </div>
                  <div
                    style={{
                      width: 20.4,
                      height: 20.4,
                      borderRadius: 10.2,
                      flexShrink: 0,
                      background: '#00FF87',
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      color: '#101813',
                    }}
                  >
                    <Icon name="plus" size={10.8} strokeWidth={2.6} />
                  </div>
                </div>
              ))}

              <div style={{ display: 'flex', alignItems: 'center', gap: 3.6 }}>
                <span style={{ color: '#B8C0BA', display: 'flex' }}>
                  <Icon name="music2" size={7.2} strokeWidth={2} />
                </span>
                  <span style={{ fontSize: 6, color: '#B8C0BA' }}>{t('phoneJoinRoom')}</span>
              </div>
            </div>

            {/* Tab bar */}
            <div
              style={{
                position: 'absolute',
                left: 7.2,
                top: 467.6,
                width: 219.6,
                height: 38.4,
                borderRadius: 10,
                background: '#121212',
                borderTop: '0.6px solid #FFFFFF14',
                padding: '4.8px 0 2.4px',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
              }}
            >
              {TABS.map((tab) => (
                <div
                  key={tab.key}
                  style={{
                    flex: 1,
                    height: '100%',
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    justifyContent: 'center',
                    gap: 3,
                  }}
                >
                  <span style={{ color: tab.color, display: 'flex' }}>
                    <Icon name={tab.icon} size={12.6} strokeWidth={2} />
                  </span>
                  <span
                    style={{
                      fontSize: 6.6,
                      fontWeight: tab.weight,
                      color: tab.labelColor,
                    }}
                  >
                    {t(tab.key)}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* Dynamic Island */}
          <div
            aria-hidden="true"
            style={{
              position: 'absolute',
              left: 89,
              top: 28,
              width: 66,
              height: 20,
              borderRadius: 10,
              background: '#000000',
            }}
          />
        </div>
      </div>

      {/* Floating chips */}
      <div
        style={{
          ...chip,
          left: 58,
          top: 150,
          transform: 'rotate(6deg)',
          transformOrigin: 'top left',
          borderRadius: 12,
          padding: '9px 11px',
          display: 'flex',
          alignItems: 'center',
          gap: 8,
        }}
      >
        <span style={{ color: '#00FF87', display: 'flex' }}>
          <Icon name="coins" size={14} strokeWidth={2} />
        </span>
        <span style={{ fontSize: 12, fontWeight: 700, color: '#F2F5F3' }}>+5 €</span>
      </div>

      <div
        style={{
          ...chip,
          left: 321,
          top: 288.51,
          transform: 'rotate(-6deg)',
          transformOrigin: 'top left',
          borderRadius: 12,
          padding: '9px 11px',
          display: 'flex',
          alignItems: 'center',
          gap: 8,
        }}
      >
        <span style={{ color: '#00FF87', display: 'flex' }}>
          <Icon name="coins" size={14} strokeWidth={2} />
        </span>
        <span style={{ fontSize: 12, fontWeight: 700, color: '#F2F5F3' }}>+10 €</span>
      </div>

      <div
        style={{
          ...chip,
          left: 11,
          top: 336.34,
          width: 176,
          transform: 'rotate(-5deg)',
          transformOrigin: 'top left',
          borderRadius: 12,
          padding: '10px 12px',
          display: 'flex',
          flexDirection: 'column',
          gap: 8,
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ color: '#00FF87', display: 'flex' }}>
            <Icon name="listMusic" size={14} strokeWidth={2} />
          </span>
          <span style={{ fontSize: 11, lineHeight: 1.27, fontWeight: 600, color: '#F2F5F3' }}>{t('phoneQueue')}</span>
          <span
            style={{
              fontSize: 11,
              lineHeight: 1.27,
              fontWeight: 700,
              color: '#00FF87',
               fontFamily: 'Inter, system-ui, sans-serif',
            }}
          >
            3
          </span>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
          {QUEUE.map(([track, artist]) => (
            <div key={track} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span style={{ fontSize: 10, lineHeight: 1.2, fontWeight: 500, color: '#F2F5F3' }}>{track}</span>
              <span style={{ fontSize: 10, lineHeight: 1.2, color: '#B8C0BA' }}>{artist}</span>
            </div>
          ))}
        </div>
      </div>

      <div
        style={{
          ...chip,
          left: 300,
          top: 70,
          transform: 'rotate(5deg)',
          transformOrigin: 'top left',
          borderRadius: 999,
          padding: '8px 12px',
          display: 'flex',
          alignItems: 'center',
          gap: 7,
        }}
      >
        <span style={{ width: 7, height: 7, borderRadius: 999, background: '#34D399', flexShrink: 0 }} />
        <span
          style={{
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: 1,
            color: '#F2F5F3',
             fontFamily: 'Inter, system-ui, sans-serif',
          }}
        >
          {t('phoneLiveGuests')}
        </span>
      </div>
    </div>
  );
}
