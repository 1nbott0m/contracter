import { ArrowRight } from 'lucide-react';
import type { MouseEvent, ReactNode } from 'react';
import type { Navigate } from '../router';

function SupportLink({ href, navigate, children }: { href: string; navigate: Navigate; children: ReactNode }) {
  const open = (event: MouseEvent<HTMLAnchorElement>) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    navigate(href);
  };
  return <a href={href} onClick={open}>{children}<ArrowRight size={15} /></a>;
}

export function SupportPage({ navigate }: { navigate: Navigate }) {
  return <article className="information-page support-page">
    <header><span className="eyebrow">SERVICE / SUPPORT</span><h1>Поддержка</h1><p>Проверки, которые доступны прямо в текущем интерфейсе.</p></header>
    <section><h2>Если данные не загрузились</h2><p>Повторите запрос кнопкой на странице. Если API вернул request ID, сохраните его вместе со временем, адресом страницы и описанием действия — эти данные помогут найти запрос в серверных журналах.</p></section>
    <section><h2>Контракт или история</h2><p>Откройте личную историю или выполните owner-scoped поиск по ID контракта. Эти инструменты не являются публичной проверкой.</p><div className="support-links"><SupportLink href="/history" navigate={navigate}>Открыть историю</SupportLink><SupportLink href="/transparency" navigate={navigate}>Проверить ID</SupportLink></div></section>
    <section><h2>Канал обращения</h2><p>Публичный канал поддержки пока не настроен. Мы не публикуем вымышленный email, чат или обещанное время ответа; перед production-запуском оператор должен добавить проверенный контакт.</p></section>
  </article>;
}
