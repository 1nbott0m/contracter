import type { MouseEvent, ReactNode } from 'react';
import type { Navigate } from '../router';

type FooterLinkProps = {
  href: string;
  navigate: Navigate;
  children: ReactNode;
};

function FooterLink({ href, navigate, children }: FooterLinkProps) {
  const open = (event: MouseEvent<HTMLAnchorElement>) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    navigate(href);
  };
  return <a href={href} onClick={open}>{children}</a>;
}

export function Footer({ navigate }: { navigate: Navigate }) {
  return <footer className="site-footer">
    <div className="footer-group footer-brand">
      <strong>CONTRACTER</strong>
      <nav aria-label="Разделы CONTRACTER">
        <FooterLink href="/contracts" navigate={navigate}>Контракты</FooterLink>
        <FooterLink href="/market" navigate={navigate}>Маркет</FooterLink>
        <FooterLink href="/inventory" navigate={navigate}>Инвентарь</FooterLink>
        <FooterLink href="/history" navigate={navigate}>История</FooterLink>
      </nav>
    </div>
    <div className="footer-group">
      <strong>ИНФОРМАЦИЯ</strong>
      <nav aria-label="Информация о сервисе">
        <FooterLink href="/transparency" navigate={navigate}>Прозрачность</FooterLink>
        <FooterLink href="/support" navigate={navigate}>Поддержка</FooterLink>
      </nav>
    </div>
    <div className="footer-group">
      <strong>ДОКУМЕНТЫ</strong>
      <nav aria-label="Документы">
        <FooterLink href="/terms" navigate={navigate}>Условия использования</FooterLink>
        <FooterLink href="/privacy" navigate={navigate}>Конфиденциальность</FooterLink>
      </nav>
      <span>Текущие границы сервиса и обработки данных описаны на связанных страницах.</span>
    </div>
    <div className="footer-group footer-check">
      <strong>ПРОВЕРКА</strong>
      <nav aria-label="Проверка контракта">
        <FooterLink href="/transparency" navigate={navigate}>Проверить честность контракта</FooterLink>
      </nav>
    </div>
  </footer>;
}
