import { useEffect, useState } from 'react';
import { getAdminToken } from '../lib/admin';

// Renders nothing until hydration, then only for admin users, so the static
// nav stays unchanged for everyone else.
export default function AdminNavLink({ mobile = false }: { mobile?: boolean }) {
  const [isAdmin, setIsAdmin] = useState(false);

  useEffect(() => {
    setIsAdmin(!!getAdminToken());
  }, []);

  if (!isAdmin) return null;

  return (
    <a
      href="/admin"
      className={
        mobile
          ? 'py-2 text-sm font-medium uppercase tracking-button text-rcommerce-accent'
          : 'text-[13px] font-medium uppercase tracking-button transition-colors hover:text-rcommerce-accent text-rcommerce-accent'
      }
    >
      Admin
    </a>
  );
}
