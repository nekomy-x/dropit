import Icon from './connect/Icons';

interface BrandLogoProps {
  size?: 'tiny' | 'small' | 'medium' | 'large';
  className?: string;
}

const LEVELS = [0.38, 0.31, 0.25, 0.2, 0.18, 0.2, 0.25, 0.33, 0.42, 0.5, 0.58, 0.65, 0.71, 0.76, 0.79, 0.77, 0.71, 0.64, 0.56, 0.48, 0.41, 0.36];

export default function BrandLogo({ size = 'medium', className = '' }: BrandLogoProps) {
  const id = `dropit-wave-${size}`;

  return (
    <span
      className={`brand-logo brand-logo-${size} ${className}`}
      role="img"
      aria-label="DROP IT"
    >
      <Icon name="headphones" size={size === 'large' ? 19 : size === 'medium' ? 16 : size === 'small' ? 14 : 9} strokeWidth={1.8} />
      <svg className="brand-logo-word" viewBox="0 0 108 27" aria-hidden="true">
        <defs>
          <linearGradient id={`${id}-mint`} x1="0" x2="1" y1="0" y2="0">
            <stop offset="0" className="brand-logo-stop-a" />
            <stop offset="1" className="brand-logo-stop-b" />
          </linearGradient>
          <mask id={`${id}-letters`}>
            <rect width="108" height="27" fill="black" />
            <text x="0" y="22" fill="white" fontFamily="Inter, sans-serif" fontSize="23" fontWeight="750" letterSpacing="-.8">DROP IT</text>
          </mask>
        </defs>
        <text className="brand-logo-letters" x="0" y="22" fontFamily="Inter, sans-serif" fontSize="23" fontWeight="750" letterSpacing="-.8">DROP IT</text>
        <g mask={`url(#${id}-letters)`}>
          {LEVELS.map((level, index) => (
            <rect
              key={index}
              x={(108 / LEVELS.length) * index}
              y={27 * (1 - level)}
              width={108 / LEVELS.length + 0.2}
              height={27 * level}
              fill={`url(#${id}-mint)`}
              className="brand-logo-bar"
              style={{ animationDelay: `${index * -73}ms` }}
            />
          ))}
        </g>
      </svg>
    </span>
  );
}
