import { NavLink, Outlet, useNavigate } from 'react-router-dom';
import { clearTokens } from '../api/client';

const NAV_SECTIONS: ReadonlyArray<{
  title: string;
  items: ReadonlyArray<{ to: string; label: string; live?: boolean }>;
}> = [
  {
    title: 'Operations',
    items: [
      { to: '/dashboard', label: 'Dashboard' },
      { to: '/clubs', label: 'Clubs' },
      { to: '/djs', label: 'DJs' },
      { to: '/guests', label: 'Guests' },
    ],
  },
  {
    title: 'Finance & safety',
    items: [
      { to: '/payouts', label: 'Payouts', live: true },
      { to: '/incidents', label: 'Incidents', live: true },
      { to: '/payments', label: 'Payments' },
      { to: '/audit-log', label: 'Audit log' },
    ],
  },
  {
    title: 'Platform',
    items: [{ to: '/platform-settings', label: 'Platform settings' }],
  },
];

/**
 * Admin shell: grouped sidebar navigation + content outlet.
 * Auth is an in-memory Bearer token (see api/client.ts); "sign out" drops
 * it. TODO(httpOnly-cookie-auth): real server-side logout once the
 * httpOnly cookie sessions land on the backend. Items marked `live` are
 * backed by implemented backend routes (F6); the rest render
 * waiting-for-backend empty states.
 */
export default function Layout() {
  const navigate = useNavigate();
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
          <span className="rounded bg-brand-600 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wider text-neutral-950">
            Admin
          </span>
        </div>

        <nav className="flex-1 px-3 py-4" aria-label="Main navigation">
          {NAV_SECTIONS.map((section) => (
            <div key={section.title} className="mb-5">
              <p className="mb-1 px-3 text-[10px] font-semibold uppercase tracking-widest text-neutral-600">
                {section.title}
              </p>
              <ul className="space-y-1">
                {section.items.map((item) => (
                  <li key={item.to} className="flex items-center gap-2">
                    <NavLink
                      to={item.to}
                      className={({ isActive }) =>
                        `block rounded-md px-3 py-2 text-sm font-medium transition-colors ${
                          isActive
                            ? 'bg-brand-600/20 text-brand-500'
                            : 'text-neutral-300 hover:bg-neutral-800 hover:text-white'
                        }`
                      }
                    >
                      {item.label}
                    </NavLink>
                    {item.live ? (
                      <span
                        className="rounded bg-green-900/60 px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider text-green-300"
                        title="Backend endpoint implemented"
                      >
                        live
                      </span>
                    ) : null}
                  </li>
                ))}
              </ul>
            </div>
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
