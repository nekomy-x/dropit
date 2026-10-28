import Icon from '../Icons';
import { useLocale } from '../../../connect/locale';

/**
 * Mint band under the hero — frame h7akdu / Ticker band.
 *
 * Seamless marquee. The track holds four identical runs and slides by exactly
 * one run (25% of the track), so the loop has no seam and three runs always
 * stay to the right of the viewport on wide screens. `ticker-band:hover`
 * pauses it, and `motion-reduce` keeps the band static with the design's
 * 20/48px inset instead of a moving full-bleed row.
 */
const RUNS = 4;

const ITEMS = {
  en: ['GUEST REQUEST', 'CLUB SETS THE PRICE', 'LESS €0.30 FEE GOES TO THE CLUB', 'DJ ACCEPTS', 'TRACK IS LIVE', 'STATS IN THE PANEL'],
  es: ['PETICIÓN DEL CLIENTE', 'EL CLUB FIJA EL PRECIO', 'MENOS 0,30 € DE SERVICIO AL CLUB', 'EL DJ ACEPTA', 'LA CANCIÓN SUENA', 'ESTADÍSTICAS EN EL PANEL'],
  fr: ['DEMANDE CLIENT', 'LE CLUB FIXE LE PRIX', 'MOINS 0,30 € DE FRAIS AU CLUB', 'LE DJ ACCEPTE', 'LE TITRE EST EN DIRECT', 'STATS DANS LE PANEL'],
  de: ['GASTWUNSCH', 'CLUB LEGT DEN PREIS FEST', 'ABZÜGLICH 0,30 € FÜR DEN CLUB', 'DJ NIMMT AN', 'TRACK LÄUFT', 'STATS IM PANEL'],
  pt: ['PEDIDO DO CLIENTE', 'O CLUBE DEFINE O PREÇO', 'MENOS 0,30 € DE SERVIÇO AO CLUBE', 'O DJ ACEITA', 'A MÚSICA TOCA', 'ESTATÍSTICAS NO PAINEL'],
  ru: ['ЗАПРОС ГОСТЯ', 'КЛУБ ЗАДАЁТ ЦЕНУ', 'ЗА ВЫЧЕТОМ 0,30 € КЛУБУ', 'ДИДЖЕЙ ПРИНИМАЕТ', 'ТРЕК В ЭФИРЕ', 'СТАТИСТИКА В ПАНЕЛИ'],
} as const;

export default function Ticker() {
  const { language } = useLocale();
  return (
    <div
      className="ticker-band flex h-12 w-full min-w-0 items-center overflow-hidden bg-drop-accent motion-reduce:px-5 lg:motion-reduce:px-12"
      aria-hidden="true"
    >
      <div className="ticker-track flex w-max">
        {Array.from({ length: RUNS }, (_, run) => (
          <div key={run} className="flex shrink-0 items-center gap-7 pr-7">
            {ITEMS[language].map((item) => (
              <span key={item} className="flex shrink-0 items-center gap-7">
                <span className="whitespace-nowrap font-mono text-[12px] font-semibold leading-[1.2] tracking-[2px] text-drop-ink">
                  {item}
                </span>
                <span className="flex text-drop-ink/60">
                  <Icon name="arrowRight" size={14} strokeWidth={2} />
                </span>
              </span>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}