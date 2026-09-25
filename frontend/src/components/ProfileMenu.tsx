import { useEffect, useRef, useState } from 'react';
import { ChevronDown, History, LogOut, Package, User } from 'lucide-react';
import type { Navigate } from '../router';
import type { SessionState } from '../session';

export function ProfileMenu({ session, navigate, logout }: { session: SessionState; navigate: Navigate; logout: () => Promise<boolean> }) {
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const outside = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, []);

  useEffect(() => {
    if (!open) return;
    const close = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      setOpen(false);
      queueMicrotask(() => triggerRef.current?.focus());
    };
    window.addEventListener('keydown', close);
    return () => window.removeEventListener('keydown', close);
  }, [open]);

  if (session.status !== 'authenticated') {
    const label = session.status === 'loading' ? 'Проверяем сессию' : session.status === 'expired' ? 'Сессия истекла' : 'Войти';
    return <button className="profile-button" type="button" disabled={session.status === 'loading'} aria-label={label} onClick={() => navigate('/login')}><span className="avatar">—</span><span>{label.toUpperCase()}</span></button>;
  }

  const accountLogin = typeof session.account.login === 'string' && session.account.login.trim()
    ? session.account.login.trim()
    : 'ПРОФИЛЬ';

  const go = (path: string) => {
    setOpen(false);
    navigate(path);
  };
  const signOut = async () => {
    if (pending) return;
    setPending(true);
    setError('');
    if (await logout()) navigate('/login', { replace: true });
    else setError('Не удалось завершить сессию.');
    setPending(false);
  };

  return <div className="profile-menu" ref={rootRef}>
    <button ref={triggerRef} id="profile-menu-trigger" className="profile-button" type="button" aria-label="Открыть меню профиля" aria-haspopup="menu" aria-controls={open ? 'profile-menu-items' : undefined} aria-expanded={open} onClick={() => setOpen((value) => !value)}><span className="avatar">{accountLogin.slice(0, 1).toUpperCase()}</span><span>{accountLogin}</span><ChevronDown size={13} /></button>
    {open && <div id="profile-menu-items" className="profile-popover" role="menu" aria-labelledby="profile-menu-trigger"><button role="menuitem" type="button" onClick={() => go('/profile')}><User size={15} /> Профиль</button><button role="menuitem" type="button" onClick={() => go('/inventory')}><Package size={15} /> Инвентарь</button><button role="menuitem" type="button" onClick={() => go('/history')}><History size={15} /> История</button><button role="menuitem" type="button" disabled={pending} onClick={() => void signOut()}><LogOut size={15} /> {pending ? 'Выходим…' : 'Выйти'}</button>{error && <p role="alert">{error}</p>}</div>}
  </div>;
}
