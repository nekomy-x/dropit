import { useEffect, useRef, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { loadStripe } from '@stripe/stripe-js/pure';
import type { Stripe, StripeElements } from '@stripe/stripe-js';
import { ArrowLeft, ArrowRight, Check, Headphones, LockKeyhole, Music2, Search, Smartphone } from 'lucide-react';
import { useLocale } from '../locale';
import { DeploymentPill } from '../components/DeploymentStatus';
import BrandLogo from '../components/BrandLogo';
import { IS_DEMO } from '../app-mode';
import { guestAppUrl, guestJoinUrl, qrToken } from '../guest/links';
import { guestCopy } from '../guest/copy';
import { confirmCheckout, GuestApiError, joinGuest, prepareCheckout, readCheckout, saveCheckout, searchGuest,
  type Checkout, type GuestSession, type GuestTrack } from '../guest/api';
import './guest-checkout.css';

const publishableKey = import.meta.env.VITE_GUEST_STRIPE_PUBLISHABLE_KEY?.trim();
let stripePromise: Promise<Stripe | null> | undefined;
function stripeClient() {
  if (!publishableKey) return Promise.resolve(null);
  return stripePromise ??= loadStripe(publishableKey).catch((error) => { stripePromise = undefined; throw error; });
}

function Artwork({ track }: { track: GuestTrack }) {
  const [failed, setFailed] = useState(false);
  return <span className="guest-art" aria-hidden="true">
    {track.artwork_url && !failed
      ? <img src={track.artwork_url} alt="" onError={() => setFailed(true)} />
      : <Music2 size={22} />}
  </span>;
}

export default function GuestCheckoutPage() {
  const [params, setParams] = useSearchParams();
  const { language, setLanguage } = useLocale();
  const copy = guestCopy[language === 'ru' ? 'ru' : 'en'];
  const token = qrToken(params.get('token') ?? '');
  const [manual, setManual] = useState('');
  const [session, setSession] = useState<GuestSession | null>(null);
  const [joining, setJoining] = useState(Boolean(token));
  const [joinAttempt, setJoinAttempt] = useState(0);
  const [error, setError] = useState('');
  const [query, setQuery] = useState('');
  const [tracks, setTracks] = useState<GuestTrack[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState(false);
  const [searchAttempt, setSearchAttempt] = useState(0);
  const [selected, setSelected] = useState<GuestTrack | null>(null);
  const [checkout, setCheckout] = useState<Checkout | null>(() => readCheckout());
  const [busy, setBusy] = useState(false);
  const [ready, setReady] = useState(false);
  const [terms, setTerms] = useState(false);
  const stripe = useRef<Stripe | null>(null);
  const elements = useRef<StripeElements | null>(null);
  const mount = useRef<HTMLDivElement>(null);
  const locked = useRef(false);
  const heading = useRef<HTMLHeadingElement>(null);
  const redirected = useRef(false);
  const activeCheckout = checkout?.sessionId === session?.session_id ? checkout : null;
  const blockedByOtherRoom = checkout && !checkout.confirmed && session && checkout.sessionId !== session.session_id;
  const paymentOpen = Boolean(activeCheckout && !activeCheckout.confirmed);
  const completed = Boolean(activeCheckout?.confirmed);
  const money = (minor: number, currency = 'EUR') => new Intl.NumberFormat(language === 'ru' ? 'ru-RU' : 'en-IE', {
    style: 'currency', currency,
  }).format(minor / 100);
  const price = activeCheckout?.intent ? money(activeCheckout.intent.amount_minor, activeCheckout.intent.currency)
    : session?.price_minor != null ? money(session.price_minor) : copy.noPrice;

  useEffect(() => {
    document.title = 'DROP IT · Quick request';
    // A room grant is established once, including React StrictMode remounts.
    let cancelled = false;
    if (!token) { setJoining(false); setSession(null); return; }
    setJoining(true); setError(''); setSession(null);
    joinGuest(token).then((joined) => {
      if (!cancelled) { setSession(joined); setSelected(null); }
    }).catch((error: unknown) => {
      if (cancelled) return;
      setError(error instanceof GuestApiError && [404, 409, 410].includes(error.status) ? copy.expired : copy.unavailable);
    }).finally(() => { if (!cancelled) setJoining(false); });
    return () => { cancelled = true; };
  }, [token, joinAttempt, copy.expired, copy.unavailable]);

  useEffect(() => {
    const controller = new AbortController();
    setTracks([]); setSearchError(false); setSearching(false);
    if (!session || query.trim().length < 2 || paymentOpen || completed) return;
    setSearching(true);
    const timer = window.setTimeout(() => {
      searchGuest(query.trim(), controller.signal).then((items) => {
        if (!controller.signal.aborted) setTracks(items);
      }).catch(() => {
        if (!controller.signal.aborted) setSearchError(true);
      }).finally(() => { if (!controller.signal.aborted) setSearching(false); });
    }, 250);
    return () => { window.clearTimeout(timer); controller.abort(); };
  }, [query, session, paymentOpen, completed, searchAttempt]);

  useEffect(() => {
    if (!paymentOpen || !activeCheckout?.intent || activeCheckout.authorized) return;
    if (activeCheckout.intent.test_mode) { setReady(true); return; }
    if (!mount.current) return;
    let cancelled = false;
    let destroy: (() => void) | undefined;
    setReady(false);
    const intent = activeCheckout.intent;
    if (!intent.client_secret || intent.client_secret.includes('_mock')) { setError(copy.notConfigured); return; }
    stripeClient().then((client) => {
      if (cancelled) return;
      if (!client) throw new Error('Stripe unavailable');
      stripe.current = client;
      const style = getComputedStyle(mount.current!);
      const instance = client.elements({ clientSecret: intent.client_secret, locale: language === 'ru' ? 'ru' : 'en', appearance: {
        theme: document.documentElement.dataset.theme === 'light' ? 'stripe' : 'night', variables: {
          colorPrimary: style.getPropertyValue('--c-accent').trim(),
          colorBackground: style.getPropertyValue('--c-surface').trim(),
          colorText: style.getPropertyValue('--c-text').trim(),
          colorDanger: style.getPropertyValue('--c-error').trim(),
          fontFamily: 'Inter, sans-serif', borderRadius: '10px',
        },
      } });
      elements.current = instance;
      const payment = instance.create('payment', { layout: 'tabs', wallets: { applePay: 'auto', googlePay: 'auto' } });
      payment.on('ready', () => { if (!cancelled) setReady(true); });
      payment.on('loaderror', () => { if (!cancelled) setError(copy.notConfigured); });
      payment.mount(mount.current!);
      destroy = () => payment.destroy();
    }).catch(() => { if (!cancelled) setError(copy.notConfigured); });
    return () => { cancelled = true; destroy?.(); elements.current = null; };
  }, [paymentOpen, activeCheckout?.intent, activeCheckout?.authorized, language, copy.notConfigured]);

  async function finalize(current: Checkout) {
    const authorized = { ...current, authorized: true };
    saveCheckout(authorized); setCheckout(authorized);
    let confirmed: Checkout;
    try { confirmed = await confirmCheckout(authorized); }
    catch (error) {
      // A capture webhook can win the race with our backend confirmation.
      // A 409 alone is never success (it can also mean a paused room).
      if (!(error instanceof GuestApiError) || error.status !== 409 || !authorized.intent || authorized.intent.test_mode) throw error;
      const client = await stripeClient();
      const result = await client?.retrievePaymentIntent(authorized.intent.client_secret);
      if (result?.paymentIntent?.status !== 'succeeded') throw error;
      confirmed = { ...authorized, confirmed: true };
      saveCheckout(confirmed);
    }
    setCheckout(confirmed);
    heading.current?.focus();
  }

  // Return from 3DS / a wallet: retrieve from Stripe rather than trusting URL
  // status parameters. The original intent and identity survive the redirect.
  useEffect(() => {
    if (!activeCheckout?.intent || !params.has('payment_intent_client_secret') || redirected.current) return;
    redirected.current = true;
    const secret = params.get('payment_intent_client_secret');
    const clean = new URLSearchParams(params);
    clean.delete('payment_intent_client_secret'); clean.delete('payment_intent'); clean.delete('redirect_status');
    setParams(clean, { replace: true });
    if (activeCheckout.intent.test_mode) { setError(copy.error); return; }
    if (secret !== activeCheckout.intent.client_secret) { setError(copy.error); return; }
    locked.current = true; setBusy(true);
    stripeClient().then(async (client) => {
      if (!client) throw new Error('Stripe unavailable');
      const result = await client.retrievePaymentIntent(secret!);
      if (result.error) throw new Error(result.error.message);
      if (!result.paymentIntent || !['requires_capture', 'succeeded', 'processing'].includes(result.paymentIntent.status)) throw new Error(copy.error);
      await finalize(activeCheckout);
    }).catch(() => setError(copy.error)).finally(() => { locked.current = false; setBusy(false); });
  }, [activeCheckout, params, setParams, copy.error]);

  async function begin() {
    if (!session || !selected || locked.current || blockedByOtherRoom) return;
    if (!publishableKey && !session.test_payments) { setError(copy.notConfigured); return; }
    locked.current = true; setBusy(true); setError('');
    try {
      const key = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Array.from(globalThis.crypto.getRandomValues(new Uint32Array(4))).join('-')}`;
      const current = activeCheckout ?? { key: `web-${key}`, sessionId: session.session_id, roomToken: token!, track: selected };
      saveCheckout(current); setCheckout(current);
      setCheckout(await prepareCheckout(current));
    } catch { setError(copy.error); }
    finally { locked.current = false; setBusy(false); }
  }

  async function pay() {
    if (!activeCheckout || locked.current || (!activeCheckout.authorized && (!terms || !ready))) return;
    if (session?.status !== 'active' && !activeCheckout.authorized) { setError(copy.paused); return; }
    locked.current = true; setBusy(true); setError('');
    try {
      if (activeCheckout.authorized) { await finalize(activeCheckout); return; }
      if (activeCheckout.intent?.test_mode) { await finalize(activeCheckout); return; }
      if (!stripe.current || !elements.current) throw new Error('Checkout unavailable');
      const returnUrl = new URL(window.location.href);
      // Keep browser 3DS returns out of the OS-associated /join path.
      // The native app has a different guest identity.
      returnUrl.pathname = '/guest/payment-return';
      returnUrl.searchParams.set('app', 'web');
      const result = await stripe.current.confirmPayment({ elements: elements.current,
        confirmParams: { return_url: returnUrl.href }, redirect: 'if_required' });
      if (result.error) { setError(result.error.message ?? copy.error); return; }
      if (!result.paymentIntent || !['requires_capture', 'processing', 'succeeded'].includes(result.paymentIntent.status)) throw new Error('Payment not authorized');
      await finalize(activeCheckout);
    } catch { setError(copy.error); }
    finally { locked.current = false; setBusy(false); }
  }

  const roomReady = session && !joining;
  return <main className="guest-checkout">
    <div className="guest-phone">
      <header className="guest-header">
        <Link to="/" className="guest-brand" aria-label="DROP IT"><BrandLogo size="medium" /></Link>
        <div className="guest-header-actions"><DeploymentPill demoBuild={IS_DEMO} /><button type="button" className="guest-language" onClick={() => setLanguage(language === 'ru' ? 'en' : 'ru')} aria-label={language === 'ru' ? 'Switch to English' : 'Переключить на русский'}>{language === 'ru' ? 'EN' : 'RU'}</button></div>
      </header>
      {token && <div className="guest-handoff">
        <span><Smartphone size={16} />{copy.quick}</span>
        {!paymentOpen && !completed && <a href={guestAppUrl(token)}>{copy.app}<ArrowRight size={16} /></a>}
      </div>}
      {roomReady && <div className="guest-room">
        <span className="guest-room-mark"><Headphones size={22} /></span>
        <div><strong>{session.club_name}</strong><p>{session.dj_display_name ?? 'DJ'} · {session.status === 'active' ? 'Live' : copy.paused}</p></div>
        <Check size={20} className="guest-accent" />
      </div>}
      <section className="guest-content" aria-busy={joining || busy}>
        {joining ? <p role="status" className="guest-notice">{copy.loading}</p> : !token ? <>
          <h1>{copy.invalid}</h1><p className="guest-muted">{copy.invalidHint}</p>
          <form className="guest-manual" onSubmit={(event) => {
            event.preventDefault(); const entered = qrToken(manual);
            if (entered) { setError(''); setParams({ token: entered, app: 'web' }); }
            else setError(copy.badCode);
          }}>
            <label htmlFor="guest-code">{copy.code}</label>
            <input id="guest-code" value={manual} onChange={(event) => setManual(event.target.value)} autoCapitalize="none" autoComplete="off" spellCheck={false} />
            <button className="guest-primary" type="submit">{copy.join}<ArrowRight size={20} /></button>
          </form>
        </> : completed ? <>
          <span className="guest-success"><Check size={32} /></span>
          <h1 ref={heading} tabIndex={-1}>{copy.sent}</h1>
          <p className="guest-muted">{activeCheckout?.intent?.test_mode ? copy.testSentHint : copy.sentHint}</p>
          <div className="guest-track-summary"><Artwork track={activeCheckout!.track} /><div><strong>{activeCheckout!.track.title}</strong><p>{activeCheckout!.track.artist}</p></div></div>
          <dl className="guest-summary"><div><dt>{copy.total}</dt><dd>{price}</dd></div><div><dt>{activeCheckout?.intent?.test_mode ? copy.testPaid : copy.processing}</dt><dd><LockKeyhole size={16} /></dd></div><div><dt>{copy.reference}</dt><dd className="guest-reference">{activeCheckout!.requestId}</dd></div></dl>
          <button type="button" className="guest-primary" onClick={() => {
            sessionStorage.removeItem('dropit.guest.checkout'); setCheckout(null); setSelected(null); setTerms(false); setQuery(''); setError('');
          }}>{copy.another}<ArrowRight size={20} /></button>
        </> : roomReady ? <>
          <h1 ref={heading} tabIndex={-1}>{paymentOpen ? copy.payment : copy.title}</h1>
          {!paymentOpen && <p className="guest-muted">{copy.subtitle}</p>}
          {blockedByOtherRoom ? <div role="alert" className="guest-error"><p>{copy.recovery}</p>{checkout.roomToken && <a className="guest-link" href={`${guestJoinUrl(checkout.roomToken)}&app=web`}>{copy.resume}</a>}</div> : paymentOpen ? <>
            <div className="guest-track-summary"><Artwork track={activeCheckout!.track} /><div><strong>{activeCheckout!.track.title}</strong><p>{activeCheckout!.track.artist}</p></div></div>
            <dl className="guest-summary"><div><dt>{copy.request}</dt><dd>{price}</dd></div><div><dt>{copy.fee}</dt><dd>{money(0)}</dd></div><div className="guest-total"><dt>{copy.total}</dt><dd>{price}</dd></div></dl>
            {activeCheckout!.authorized ? <p className="guest-notice">{copy.processing}</p> : <>
              {activeCheckout!.intent?.test_mode ? <div className="guest-test-payment" role="group" aria-label={copy.testTitle}><strong><Check size={18} />{copy.testTitle}</strong><p>{copy.testHint}</p></div> : <>
                <p className="guest-security"><LockKeyhole size={16} />{copy.secure}</p>
                <div ref={mount} className="guest-payment-element" />
              </>}
              {!ready && <p role="status" className="guest-muted">{copy.preparing}</p>}
              <label className="guest-terms"><input type="checkbox" checked={terms} onChange={(event) => setTerms(event.target.checked)} /><span>{copy.terms}</span></label>
            </>}
            {(!activeCheckout!.intent || (!activeCheckout!.intent.test_mode && !activeCheckout!.intent.client_secret)) && <button type="button" className="guest-primary" disabled={busy} onClick={() => {
              if (locked.current) return;
              locked.current = true; setBusy(true); setError('');
              prepareCheckout(activeCheckout!).then(setCheckout).catch(() => setError(copy.error)).finally(() => { locked.current = false; setBusy(false); });
            }}>{copy.resume}<ArrowRight size={20} /></button>}
          </> : <>
            <label className="guest-search"><Search size={20} /><span className="sr-only">{copy.search}</span><input type="search" value={query} placeholder={copy.search} onChange={(event) => setQuery(event.target.value)} autoComplete="off" /></label>
            <div className="guest-results" role="group" aria-label={copy.choose}>
              {searching ? <p role="status" className="guest-notice">{copy.searching}</p> : searchError ? <div role="alert"><p>{copy.searchError}</p><button className="guest-link" onClick={() => setSearchAttempt((attempt) => attempt + 1)}>{copy.retry}</button></div>
                : tracks.length ? tracks.map((track) => <button type="button" className={`guest-track ${selected?.id === track.id ? 'is-selected' : ''}`} aria-pressed={selected?.id === track.id} key={track.id} onClick={() => { setSelected(track); setError(''); }}>
                  <Artwork track={track} /><span className="guest-track-copy"><strong>{track.title}</strong><span>{track.artist}</span></span><span className="guest-radio">{selected?.id === track.id && <Check size={16} />}</span>
                </button>) : <p className="guest-notice">{query.trim().length < 2 ? copy.searchHint : copy.empty}</p>}
            </div>
          </>}
        </> : null}
        {error && <div role="alert" className="guest-error"><p>{error}</p>{!session && token && <button type="button" className="guest-link" onClick={() => setJoinAttempt((attempt) => attempt + 1)}>{copy.retry}</button>}</div>}
      </section>
      {roomReady && !completed && !blockedByOtherRoom && <footer className="guest-footer">
        {!paymentOpen && selected && <div className="guest-footer-track"><div><span>{copy.selected}</span><strong>{selected.title}</strong><small>{selected.artist}</small></div><strong>{price}</strong></div>}
        {paymentOpen ? activeCheckout?.intent && <button type="button" className="guest-primary" disabled={busy || (!activeCheckout.authorized && (!terms || !ready || session.status !== 'active'))} onClick={() => { void pay(); }}>{busy ? copy.paying : activeCheckout.authorized ? copy.resume : `${activeCheckout.intent.test_mode ? copy.testPay : copy.pay} ${price}`}<LockKeyhole size={18} /></button>
          : <button type="button" className="guest-primary" disabled={!selected || busy || session.status !== 'active' || session.price_minor == null} onClick={() => { void begin(); }}>{busy ? copy.preparing : copy.checkout}<ArrowRight size={20} /></button>}
        <p><LockKeyhole size={13} />{copy.browser}</p>
      </footer>}
      {!token && <Link to="/" className="guest-home"><ArrowLeft size={16} />DROP IT</Link>}
    </div>
  </main>;
}
