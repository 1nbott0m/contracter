import type { Navigate } from '../router';
import type { SessionState } from '../session';
import { VerificationPage } from './VerificationPage';

export function TransparencyPage({ session, navigate }: { session: SessionState; navigate: Navigate }) {
  return <div className="transparency-page">
    <section className="intro commerce-intro"><div><span className="eyebrow">TRANSPARENCY / KNOWN BOUNDARIES</span><h1>Прозрачность</h1><p>Только проверяемые свойства текущей реализации — без недоступных публичных доказательств.</p></div></section>
    <section className="transparency-facts" aria-label="Проверяемые свойства">
      <article className="panel"><h2>4–10 входов</h2><p>Интерфейс и backend принимают контракт только в поддерживаемом диапазоне предметов.</p></article>
      <article className="panel"><h2>Ответ сервера</h2><p>Результат считается принятым только после успешного ответа API. Ошибка не заменяется локальным результатом.</p></article>
      <article className="panel"><h2>Приватная история</h2><p>Доступный поиск проверяет ID только в owner-scoped истории текущего аккаунта.</p></article>
      <article className="panel"><h2>Граница проверки</h2><p>Текущий поиск не является публичной криптографической проверкой. Публичный proof endpoint backend не предоставляет.</p></article>
    </section>
    <VerificationPage session={session} navigate={navigate} embedded />
  </div>;
}
