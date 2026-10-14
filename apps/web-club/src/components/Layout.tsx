import { NavLink, Outlet, useNavigate } from 'react-router-dom';
import { clearTokens } from '../api/client';

const NAV_ITEMS: ReadonlyArray<{ to: string; label: string }> = [
  { to: '/dashboard', label: 'Dashboard' },
  { to: '/sessions', label: 'Sessions' },
  { to: '/pricing', label: 'Pricing' },
  { to: '/djs', label: 'DJs' },
  { to: '/earnings', label: 'Earnings' },
];

/**
 * Authenticated shell: sidebar navigation + content outlet.
 * Auth is an in-memory Bearer token (see api/client.ts); "sign out" drops
 * it. TODO(httpOnly-cookie-auth): real server-side logout once the
 * httpOnly cookie sessions land on the backend.
 */
export default function Layout() {
  const navigate = useNavigate();

  // Minimal guard: without a token the API returns 401 and pages redirect
  // to /login themselves; here we just wire the manual "sign out" action.
  const signOut = () => {
    clearTokens();
    navigate('/login', { replace: true });
  };

  return (
    <div className="flex min-h-screen">
      <aside className="flex w-60 shrink-0 flex-col border-r border-neutral-800 bg-neutral-900">
        <div className="flex h-16 items-center gap-2 px-5">
          <span className="text-lg font-black tracking-tight text-white">
            DROP IT
          </span>
          <span className="rounded bg-brand-700 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wider text-white">
            Club
          </span>
        </div>

        <nav className="flex-1 space-y-1 px-3 py-4" aria-label="Main navigation">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              className={({ isActive }) =>
                `block rounded-md px-3 py-2 text-sm font-medium transition-colors ${
                  isActive
                    ? 'bg-brand-700 text-white'
                    : 'text-neutral-300 hover:bg-neutral-800 hover:text-white'
                }`
              }
            >
              {item.label}
            </NavLink>
          ))}
        </nav>

        <div className="border-t border-neutral-800 p-3">
          <button
            type="button"
            onClick={signOut}
            className="w-full rounded-md px-3 py-2 text-left text-sm font-medium text-neutral-400 transition-colors hover:bg-neutral-800 hover:text-white"
          >
            Sign out
          </button>
        </div>
      </aside>

      <main className="flex-1 overflow-y-auto p-8">
        <Outlet />
      </main>
    </div>
  );
}
