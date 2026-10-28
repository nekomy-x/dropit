# i18n brief — translating a DROP IT panel page

You are translating ONE page of the DROP IT web panel into the six product
languages. Everything you need is below; do not invent a different pattern.

## The infrastructure (already built — do not modify it)

- `apps/web-club/src/i18n.tsx` — the provider. `useLocale()` returns
  `{ language, setLanguage, locale, t, has }`. `t(key, vars?)` interpolates
  `{name}` placeholders. English is the fallback: a key missing from a
  translation renders the English string.
- `apps/web-club/src/locale.tsx` — mounts the provider with a merged catalog.
- `apps/web-club/src/messages/index.ts` — the area registry. Every area module
  is listed in `AREAS`; a duplicate key across modules is reported as a problem.
- `apps/web-club/src/messages/<area>.ts` — one catalog per screen area, shape:
  `export const <area>Catalog = { en: {...}, es: {...}, fr: {...}, de: {...}, pt: {...}, ru: {...} } satisfies MessageCatalog;`
- Shared shell keys (`account.*`, `nav.*`, `common.*`, `shell.*`) live in
  `packages/ui/messages.ts` and are already merged. **Do not redeclare them.**
  Reuse them where they fit (`common.refresh`, `common.tryAgain`,
  `common.unavailable`, `common.loading`, `common.saving`, `common.cancel`,
  `common.optional`, `common.none`, `common.all`).

## Rules

1. **Keys** are `<area>.<camelCase>` — e.g. `pricing.currentPrice`,
   `sessions.closeShift`. Never reuse a key for two different sentences.
2. **Every key must exist in all six languages.** Write real translations, not
   English copies, for es / fr / de / pt / ru. Keep the register of the existing
   catalog: operational, calm, second person, no marketing, no exclamation marks.
   The `ru` catalog is the one the product owner reads first — make it natural
   Russian, not a calque.
3. **Placeholders** (`{count}`, `{name}`) must survive unchanged in every
   language. Never bake a formatted number or currency into a string: format it
   in the component with `Intl` using `locale` from `useLocale()` and pass it in
   as a variable.
4. **Money and numbers** use the active locale:
   `new Intl.NumberFormat(locale, { style: 'currency', currency: 'EUR' })`.
   Remove hardcoded `'en-IE'` / `'en-GB'` formatters from the page you touch.
5. **Do not change behaviour, layout, class names, API calls or state logic.**
   Only replace user-facing strings with `t(...)` and route formatters through
   `locale`. Keep `aria-label`, `title` and `placeholder` translated too — they
   are read aloud.
6. **No literal English left in JSX text**, `aria-label`, `title`, `placeholder`,
   `alt`, or toast/notice strings. Debug codes, HTTP status strings and API
   field names stay English.
7. The page must still typecheck with `npx tsc --noEmit`.

## How to apply it in a page

```tsx
import { useLocale } from '../locale';

export default function PricingPage() {
  const { t, locale } = useLocale();
  ...
  return <h1>{t('pricing.title')}</h1>;
}
```

For a pure helper module that produces copy (like
`components/club/board/nextActions.ts`), take a translator argument:
`function derive(input, t: (key: string) => string)`.

## Verification before you report back

```bash
cd apps/web-club && npx tsc --noEmit -p tsconfig.json 2>&1 | grep -v "components/ui/charts"
```

Only errors in `components/ui/charts.tsx`, `components/ui/Select.tsx` or
`components/connect/landing/*` are foreign to you — another change is in flight
there; ignore those and fix everything in your own files.

Then confirm your page renders by visiting it in the demo dev server (already
running on http://localhost:5180) with the language preset:

```js
// in the browser console
localStorage.setItem('dropit.language', 'ru'); location.reload();
```

## Reporting

Report: the files you changed, the number of keys you added per language, and
anything you had to leave untranslated with the reason. Do not report progress
mid-way; report once at the end.
