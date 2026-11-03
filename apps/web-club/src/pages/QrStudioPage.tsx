/**
 * QR generator — one code, styled, downloadable.
 *
 * Scope is deliberately small, because the previous version of this screen tried
 * to be a print shop: it composed a whole laminated sign (layouts, paper sizes,
 * five lines of caption text, brand marks). That is not what a club asks for. It
 * asks for **the QR image**: dots it likes, colours it uses, its own logo in the
 * middle, and a file it can drop into whatever template it already has.
 *
 * So this page is one shape of screen:
 *
 *   [ the code, big, always visible ]   [ link · dots · colours · mark · download ]
 *
 * Everything on the right changes something you can see on the left, and nothing
 * here draws text onto the code itself.
 *
 * Two behaviours are not cosmetic and are kept from the earlier version because
 * they are the difference between a file that scans and a file that does not:
 *
 *  - a centre mark forces error correction to H (measured: at H a mark up to 28%
 *    of the width still decodes; at Q it stops at 22%);
 *  - the page decodes its own output with jsQR and says so, so an unreadable
 *    design is caught here instead of on the bar.
 */
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import PageHeading from '../components/PageHeading';
import { Panel, ToastRegion, useToasts } from '../components/club';
import Select from '../components/ui/Select';
import QrSignPreview from '../components/qr/QrSignPreview';
import QrIcon from '../components/qr/QrIcon';
import { ChoiceTile, ColorField, RangeField } from '../components/qr/QrControls';
import { useLocale } from '../locale';
import { useAsyncData } from '../hooks/useAsyncData';
import { generateSessionQr, getClubDashboard, type QrGenerateResponse } from '../api/client';
import { encodeQr, QrEncodeError } from '../qr/encode';
import { qrSymbolSvg, LOGO_MAX_SCALE } from '../qr/geometry';
import { PRESETS } from '../qr/presets';
import { createDoc, qrReducer, requiredEcc } from '../qr/studio-state';
import { contrastRatio, formatRatio, isDarker } from '../qr/color';
import { downloadBlob, exportPdf, exportRaster, exportSvg, printCard } from '../qr/export';
import { decodeSvg, type DecodeResult } from '../qr/decode-check';
import type { ColorMode, Ecc, EyeCenter, EyeFrame, ModuleShape, QrStyle } from '../qr/types';
import './qr-studio.css';
import { guestJoinUrl } from '../guest/links';

/** The square print size used for PDF and the print dialog. */
const PRINT_SIZE = 'sticker' as const;
/** PNG/JPG pixel sizes. 1024 is enough for a table card and prints at 325 dpi. */
const PIXEL_SIZES = [512, 1024, 2048] as const;

const SHAPES: ReadonlyArray<{ id: ModuleShape; key: string }> = [
  { id: 'square', key: 'qr.shapeSquare' },
  { id: 'rounded', key: 'qr.shapeRounded' },
  { id: 'dot', key: 'qr.shapeDot' },
  { id: 'connected', key: 'qr.shapeJoined' },
  { id: 'liquid', key: 'qr.shapeLiquid' },
];

const EYE_FRAMES: ReadonlyArray<EyeFrame> = ['square', 'rounded', 'circle', 'leaf'];
const EYE_CENTERS: ReadonlyArray<EyeCenter> = ['square', 'rounded', 'dot'];
const COLOR_MODES: ReadonlyArray<ColorMode> = ['single', 'gradient'];

function roomUrl(): string {
  const origin = typeof window === 'undefined' ? '' : window.location.origin;
  return `${origin}/room`;
}

function slug(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 32) || 'qr';
}

export default function QrStudioPage() {
  const { t } = useLocale();
  const { toasts, push, dismiss } = useToasts();

  const url = roomUrl();
  const [doc, dispatch] = useReducer(qrReducer, undefined, () => createDoc(url, url));
  const [pixels, setPixels] = useState<number>(1024);

  // --- shift token -------------------------------------------------------
  const loadShift = useCallback(() => getClubDashboard().then((board) => board.session).catch(() => null), []);
  const { data: shift, loading: shiftLoading } = useAsyncData(loadShift);
  const [token, setToken] = useState<QrGenerateResponse | null>(null);
  const [tokenBusy, setTokenBusy] = useState(false);
  const shiftOpen = Boolean(shift && shift.status !== 'closed');

  const useToken = async () => {
    if (!shift || tokenBusy) return;
    setTokenBusy(true);
    try {
      const issued = await generateSessionQr(shift.id);
      setToken(issued);
      dispatch({ type: 'value', value: guestJoinUrl(issued.qr_token) });
    } catch {
      push({ title: t('qr.failed'), tone: 'error' });
    } finally {
      setTokenBusy(false);
    }
  };

  // --- encode ------------------------------------------------------------
  const encoded = useMemo(() => {
    try {
      return { grid: encodeQr(doc.value, { ecc: doc.encode.ecc, maskPattern: doc.encode.maskPattern }), error: null as string | null };
    } catch (error) {
      return { grid: null, error: error instanceof QrEncodeError ? error.message : t('qr.failed') };
    }
  }, [doc.value, doc.encode.ecc, doc.encode.maskPattern, t]);

  // --- the drawing -------------------------------------------------------
  // One string drives the preview, every download and the scan check, so the
  // file a user gets is exactly what they approved on screen.
  const svg = useMemo(() => {
    if (!encoded.grid) return '';
    try {
      return qrSymbolSvg(encoded.grid, doc.style, {
        idPrefix: 'qr',
        sizePx: 1024,
        title: t('qr.title'),
      });
    } catch {
      return '';
    }
  }, [encoded.grid, doc.style, t]);

  // --- scan check --------------------------------------------------------
  const [decode, setDecode] = useState<DecodeResult>({ state: 'unknown' });
  useEffect(() => {
    if (!svg) {
      setDecode({ state: 'unknown' });
      return undefined;
    }
    let active = true;
    setDecode({ state: 'checking' });
    const timer = window.setTimeout(() => {
      void decodeSvg(svg, doc.value).then((result) => { if (active) setDecode(result); });
    }, 300);
    return () => { active = false; window.clearTimeout(timer); };
  }, [svg, doc.value]);

  // --- export ------------------------------------------------------------
  const [busy, setBusy] = useState<string | null>(null);
  const base = slug(`${t('qr.title')}-${doc.value.length}`);
  const run = async (kind: string, work: () => Promise<void> | void) => {
    if (busy) return;
    setBusy(kind);
    try {
      await work();
    } catch {
      push({ title: t('qr.failed'), tone: 'error' });
    } finally {
      setBusy(null);
    }
  };

  const doSvg = () => run('svg', () => {
    const result = exportSvg(svg, `${base}.svg`);
    downloadBlob(result.blob, result.filename);
  });
  const doRaster = (format: 'png' | 'jpg') => run(format, async () => {
    const result = await exportRaster(svg, {
      format,
      widthPx: pixels,
      heightPx: pixels,
      filename: `${base}.${format}`,
      background: doc.style.colors.bg,
    });
    downloadBlob(result.blob, result.filename);
  });
  const doPdf = () => run('pdf', async () => {
    const result = await exportPdf(svg, {
      size: PRINT_SIZE,
      filename: `${base}.pdf`,
      title: t('qr.title'),
      dpi: 300,
    });
    downloadBlob(result.blob, result.filename);
  });
  const doPrint = () => run('print', () => printCard(svg, PRINT_SIZE));

  // --- mark --------------------------------------------------------------
  const fileRef = useRef<HTMLInputElement | null>(null);
  const onFile = (file: File | undefined) => {
    if (!file) return;
    if (file.size > 512 * 1024) {
      push({ title: t('qr.logoTooBig'), tone: 'error' });
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      const value = typeof reader.result === 'string' ? reader.result : '';
      if (!value) return;
      const current = doc.style.logo;
      dispatch({
        type: 'logo',
        logo: {
          kind: 'image',
          value,
          scale: current?.scale ?? 0.2,
          padding: current?.padding ?? 0.1,
          radius: current?.radius ?? 0.18,
          plate: current?.plate ?? true,
          plateColor: current?.plateColor ?? doc.style.colors.bg,
        },
      });
    };
    reader.readAsDataURL(file);
  };

  // --- facts the screen states ------------------------------------------
  const logo = doc.style.logo;
  const contrast = contrastRatio(doc.style.colors.fg, doc.style.colors.bg);
  const inverted = !isDarker(doc.style.colors.fg, doc.style.colors.bg);
  const gradient = doc.style.colorMode === 'gradient';
  const eccFloor = requiredEcc(doc.style);

  const applyPreset = (id: string) => dispatch({ type: 'preset', id });

  return (
    <div className="qr-page">
      <PageHeading
        title={t('qr.title')}
        action={(
          <button
            type="button"
            className="club-button club-button-primary"
            onClick={() => { void doPrint(); }}
            disabled={!svg || busy !== null}
            data-control="button"
          >
            <QrIcon name="print" />{t('qr.print')}
          </button>
        )}
      />

      <div className="qr-layout">
        {/* ---- the code ------------------------------------------------ */}
        <div className="qr-stage">
          <div className="qr-frame">
            {svg ? (
              <QrSignPreview svg={svg} label={t('qr.title')} edge={false} />
            ) : (
              <p className="qr-empty">{encoded.error ?? t('qr.failed')}</p>
            )}
          </div>

          <div className="qr-stage-foot">
            <span className={`qr-scan is-${decode.state}`} role="status" aria-live="polite">
              <QrIcon name={decode.state === 'failed' ? 'inspect' : 'scan'} />
              {decode.state === 'ok'
                ? t('qr.scanOk')
                : decode.state === 'failed'
                  ? t('qr.scanFail')
                  : t('qr.scanChecking')}
            </span>
            <button
              type="button"
              className="club-button club-button-quiet"
              onClick={() => dispatch({ type: 'resetDoc' })}
              data-control="button"
            >
              {t('qr.reset')}
            </button>
          </div>
        </div>

        {/* ---- the controls ------------------------------------------- */}
        <div className="qr-side">
          {inverted ? <p className="qr-note is-error">{t('qr.inverted')}</p> : null}

          {/* link */}
          <Panel title={t('qr.linkPanel')} detail={t('qr.linkPanelDetail')}>
            <label className="field">
              <span className="field-label">{t('qr.linkValue')}</span>
              <input
                className="field-input is-mono"
                aria-label={t('qr.linkValue')}
                value={doc.value}
                onChange={(event) => dispatch({ type: 'value', value: event.target.value })}
                spellCheck={false}
                autoComplete="off"
              />
            </label>
            <div className="qr-row">
              <button
                type="button"
                className="club-button club-button-quiet"
                onClick={() => dispatch({ type: 'value', value: url })}
                data-control="button"
              >
                {t('qr.linkRoom')}
              </button>
              <button
                type="button"
                className="club-button club-button-quiet"
                onClick={() => { void useToken(); }}
                disabled={!shiftOpen || tokenBusy}
                data-control="button"
              >
                {t('qr.linkToken')}
              </button>
              <span className="qr-meta">{t('qr.linkChars', { count: doc.value.length })}</span>
            </div>
            {!shiftLoading && !shiftOpen ? <p className="qr-meta">{t('qr.linkNoShift')}</p> : null}
            {token ? <p className="qr-note is-warn">{t('qr.linkNote')}</p> : null}
          </Panel>

          {/* dots */}
          <Panel title={t('qr.stylePanel')}>
            <div className="qr-presets">
              {PRESETS.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  className={`qr-swatch ${doc.card.preset === preset.id ? 'is-active' : ''}`}
                  onClick={() => applyPreset(preset.id)}
                  aria-pressed={doc.card.preset === preset.id}
                  title={preset.name}
                  data-control="button"
                >
                  <span
                    className="qr-swatch-dots"
                    style={{
                      background: preset.style.colors.bg,
                      color: preset.style.colors.fg,
                    }}
                    aria-hidden="true"
                  >
                    <span style={{ background: preset.style.colors.fg }} />
                    <span style={{ background: preset.style.colors.eye }} />
                  </span>
                  <span className="qr-swatch-name">{t(`qr.preset${preset.id.charAt(0).toUpperCase()}${preset.id.slice(1)}`)}</span>
                </button>
              ))}
            </div>

            <div className="qr-shapes">
              {SHAPES.map((shape) => (
                <ChoiceTile
                  key={shape.id}
                  name="shape"
                  label={t(shape.key)}
                  selected={doc.style.modules === shape.id}
                  onSelect={() => dispatch({ type: 'moduleShape', shape: shape.id })}
                />
              ))}
            </div>

            <div className="qr-grid">
              <RangeField
                label={t('qr.corner')}
                value={doc.style.moduleRadius}
                min={0}
                max={0.5}
                step={0.05}
                onChange={(value) => dispatch({ type: 'style', patch: { moduleRadius: value } })}
                format={(value) => `${Math.round((value / 0.5) * 100)}%`}
              />
              <RangeField
                label={t('qr.gap')}
                value={doc.style.moduleGap}
                min={0}
                max={0.15}
                step={0.01}
                onChange={(value) => dispatch({ type: 'style', patch: { moduleGap: value } })}
                format={(value) => `${Math.round((value / 0.15) * 100)}%`}
              />
              <div className="qr-field">
                <span className="field-label">{t('qr.eyeFrame')}</span>
                <Select
                  ariaLabel={t('qr.eyeFrame')}
                  options={EYE_FRAMES.map((frame) => ({ value: frame, label: frame }))}
                  value={doc.style.eyeFrame}
                  onChange={(value) => dispatch({ type: 'eyeFrame', frame: value as EyeFrame })}
                />
              </div>
              <div className="qr-field">
                <span className="field-label">{t('qr.eyeCenter')}</span>
                <Select
                  ariaLabel={t('qr.eyeCenter')}
                  options={EYE_CENTERS.map((center) => ({ value: center, label: center }))}
                  value={doc.style.eyeCenter}
                  onChange={(value) => dispatch({ type: 'eyeCenter', center: value as EyeCenter })}
                />
              </div>
            </div>
            <p className="qr-meta">{t('qr.shapeHint')}</p>
          </Panel>

          {/* colours */}
          <Panel
            title={t('qr.colorPanel')}
            action={<span className={`qr-chip ${inverted ? 'is-bad' : contrast >= 7 ? 'is-ok' : 'is-warn'}`}>{t('qr.contrast', { ratio: formatRatio(contrast) })}</span>}
          >
            <div className="qr-grid">
              <ColorField
                label={t('qr.ink')}
                value={doc.style.colors.fg}
                onChange={(value) => dispatch({ type: 'colors', patch: { fg: value } })}
              />
              <ColorField
                label={t('qr.paper')}
                value={doc.style.colors.bg}
                onChange={(value) => dispatch({ type: 'colors', patch: { bg: value } })}
              />
              <ColorField
                label={t('qr.eyeColor')}
                value={doc.style.colors.eye}
                onChange={(value) => dispatch({ type: 'colors', patch: { eye: value } })}
              />
              <ColorField
                label={t('qr.eyeCenterColor')}
                value={doc.style.colors.eyeCenter}
                onChange={(value) => dispatch({ type: 'colors', patch: { eyeCenter: value } })}
              />
            </div>

            <div className="qr-row">
              <Select
                size="sm"
                className="qr-mode"
                ariaLabel={t('qr.colorPanel')}
                options={COLOR_MODES.map((mode) => ({
                  value: mode,
                  label: mode === 'gradient' ? t('qr.gradient') : t('qr.ink'),
                }))}
                value={doc.style.colorMode === 'gradient' ? 'gradient' : 'single'}
                onChange={(value) => dispatch({ type: 'colorMode', mode: value as ColorMode })}
              />
              <button
                type="button"
                className="club-button club-button-quiet"
                onClick={() => {
                  const ink = doc.style.colors.fg;
                  const paper = doc.style.colors.bg;
                  dispatch({ type: 'colors', patch: { fg: paper, bg: ink } });
                }}
                data-control="button"
              >
                {t('qr.swap')}
              </button>
            </div>

            {gradient ? (
              <div className="qr-grid">
                <ColorField
                  label={t('qr.ink')}
                  value={doc.style.colors.fg2}
                  onChange={(value) => dispatch({ type: 'colors', patch: { fg2: value } })}
                />
                <RangeField
                  label={t('qr.angle')}
                  value={doc.style.gradientAngle}
                  min={0}
                  max={360}
                  step={5}
                  onChange={(value) => dispatch({ type: 'style', patch: { gradientAngle: value } })}
                  format={(value) => `${value}°`}
                />
              </div>
            ) : null}

            <p className={`qr-note ${inverted ? 'is-error' : contrast >= 7 ? 'is-ok' : 'is-warn'}`}>
              {inverted ? t('qr.inverted') : contrast >= 7 ? t('qr.contrastOk') : t('qr.contrastWarn')}
            </p>
          </Panel>

          {/* mark */}
          <Panel title={t('qr.logoPanel')}>
            <div className="qr-row">
              <button
                type="button"
                className="club-button"
                onClick={() => fileRef.current?.click()}
                data-control="button"
              >
                <QrIcon name="image" />{t('qr.logoUpload')}
              </button>
              {logo ? (
                <button
                  type="button"
                  className="club-button club-button-quiet"
                  onClick={() => dispatch({ type: 'logo', logo: null })}
                  data-control="button"
                >
                  {t('qr.logoRemove')}
                </button>
              ) : (
                <span className="qr-meta">{t('qr.logoNone')}</span>
              )}
              <input
                ref={fileRef}
                type="file"
                accept="image/png,image/svg+xml,image/jpeg,image/webp"
                className="qr-file"
                tabIndex={-1}
                aria-hidden="true"
                onChange={(event) => { onFile(event.target.files?.[0]); event.target.value = ''; }}
              />
            </div>
            {logo ? (
              <RangeField
                label={t('qr.logoSize')}
                value={logo.scale}
                min={0.1}
                max={LOGO_MAX_SCALE}
                step={0.01}
                onChange={(value) => dispatch({ type: 'logo', logo: { ...logo, scale: value } })}
                format={(value) => `${Math.round(value * 100)}%`}
              />
            ) : null}
            <p className="qr-meta">{t('qr.logoHint')}</p>
          </Panel>

          {/* download */}
          <Panel title={t('qr.export')}>
            <div className="qr-grid">
              <div className="qr-field">
                <span className="field-label">{t('qr.resolution')}</span>
                <Select
                  ariaLabel={t('qr.resolution')}
                  options={PIXEL_SIZES.map((size) => ({ value: String(size), label: `${size} × ${size}` }))}
                  value={String(pixels)}
                  onChange={(value) => setPixels(Number(value))}
                />
              </div>
              <div className="qr-field">
                <span className="field-label">{t('qr.linkValue')}</span>
                <span className="qr-meta is-mono">
                  {encoded.grid
                    ? `${t('qr.linkChars', { count: doc.value.length })} · ${encoded.grid.size}×${encoded.grid.size}`
                    : '—'}
                </span>
              </div>
            </div>
            <div className="qr-downloads">
              <button type="button" className="club-button club-button-primary" onClick={() => { void doRaster('png'); }} disabled={!svg || busy !== null} data-control="button">
                {busy === 'png' ? t('qr.working') : 'PNG'}
              </button>
              <button type="button" className="club-button" onClick={() => { void doSvg(); }} disabled={!svg || busy !== null} data-control="button">
                {busy === 'svg' ? t('qr.working') : 'SVG'}
              </button>
              <button type="button" className="club-button" onClick={() => { void doRaster('jpg'); }} disabled={!svg || busy !== null} data-control="button">
                {busy === 'jpg' ? t('qr.working') : 'JPG'}
              </button>
              <button type="button" className="club-button" onClick={() => { void doPdf(); }} disabled={!svg || busy !== null} data-control="button">
                {busy === 'pdf' ? t('qr.working') : 'PDF'}
              </button>
            </div>
            <div className="qr-row">
              <button type="button" className="club-button club-button-quiet" onClick={() => { void doPrint(); }} disabled={!svg || busy !== null} data-control="button">
                <QrIcon name="print" />{t('qr.print')}
              </button>
              <Link className="club-button club-button-quiet" to="/sessions" data-control="button">
                {t('nav.liveRoom')}
              </Link>
            </div>
          </Panel>

          <div className="qr-row">
            <Select
              size="sm"
              className="qr-mode"
              ariaLabel="Error correction"
              options={(['L', 'M', 'Q', 'H'] as Ecc[]).map((level) => ({ value: level, label: `ECC ${level}` }))}
              value={doc.encode.ecc}
              onChange={(value) => dispatch({ type: 'ecc', ecc: value as Ecc })}
              disabled={Boolean(eccFloor)}
            />
          </div>
        </div>
      </div>

      <ToastRegion toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}

/** Exported for the stylesheet's own reference; not used at runtime. */
export type { QrStyle };
