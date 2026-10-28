# DROP IT project instructions

## Product surface

DROP IT is an operational product for club staff, DJs, and platform operators.
Most web screens are working tools used under time pressure. Optimize for scanability,
clear state, fast action, and recovery from errors. Do not treat operational screens as
marketing pages, portfolios, or visual experiments.

## Design authority

1. Preserve the existing product direction and implemented design system unless the user
   explicitly asks for a redesign. The current system is `docs/ui/PANEL-DESIGN.md`, built
   from `packages/ui/` (tokens, `panel.css`, Icon, theme, charts, StatCard). Never edit the
   copies inside `apps/*/src/styles/` — edit `packages/ui/` and run
   `node scripts/sync-ui-tokens.mjs`, or `--check` will fail the gate.
2. Read `PRODUCT.md` and `DESIGN.md` before changing a web or mobile UI. Read the
   relevant screen spec in `docs/ui/` when implementing a specified flow.
3. The design reference in `docs/ui/reference/` is a source of constraints, not a reason
   to invent extra decoration. When code and documentation disagree, mention the conflict
   and follow the user's latest instruction.
4. Do not select a new visual direction, font, palette, layout archetype, or animation
   system by guessing, randomization, or a generic design skill.

## UI rules

- Use one coherent token system per surface. Reuse existing tokens before adding new ones.
- Every authenticated screen must work in both themes (`<html data-theme="dark|light">`).
  A raw colour literal, a `--admin-*` / `--ops-*` / `--dj-*` colour, or a Tailwind
  `text-neutral-*` in new markup is a defect: use `var(--c-*)` or the `panel-*` classes.
- Verify UI work with `node scripts/panel-audit.mjs` (both themes, every route) and
  `node scripts/panel-shot.mjs` for screenshots; `apps/web-club` additionally has
  `npm run ui:check` (16-assert acceptance gate).
- Keep operational layouts compact and information-led. Put the current state and next
  action first.
- Cards are for genuinely grouped or independently actionable content. Prefer spacing,
  dividers, tables, and plain sections when a container adds no meaning.
- Do not nest cards or create decorative card stacks.
- Do not use gradients, glow, grain, glass effects, bento grids, parallax, scroll hijacks,
  random layout variation, or decorative illustrations unless the user explicitly asks for
  that treatment and it serves the task.
- Do not use oversized hero sections, marketing copy, fake metrics, or Awwwards-style
  composition in authenticated product surfaces.
- Use restrained radii from the existing system. Pills are for compact status labels or
  controls that are explicitly defined as pills; they are not the default shape for every
  button, panel, or input.
- Motion is optional. Add it only when it communicates state or improves feedback. Respect
  reduced motion and never make content depend on an entrance animation.
- Use readable product typography. Do not replace an existing font with a fashionable
  display font merely to make a screen look more distinctive.
- Keep accessibility, keyboard focus, contrast, touch targets, responsive behavior, and
  status text ahead of decoration.

## Implementation discipline

- Inspect the target screen and neighboring components before editing.
- Keep changes scoped to the requested surface. Do not rewrite unrelated screens or tokens.
- Do not replace working UI with placeholders or mock content.
- Preserve API behavior and existing navigation while changing presentation.
- Verify changed web apps with their package scripts and, when practical, a browser
  screenshot at desktop and mobile widths.

## Anti-pattern review

Before presenting UI work, check for: nested cards, excessive rounded corners, gradients,
glows, decorative motion, tiny low-contrast text, repeated uppercase eyebrows, unexplained
icons, dashboard-as-landing-page composition, and any visual choice that cannot be tied to
the product task.
