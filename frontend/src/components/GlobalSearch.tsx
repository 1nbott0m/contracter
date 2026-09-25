import { useEffect, useMemo, useRef, useState } from 'react';
import { Search, X } from 'lucide-react';
import { api, type CatalogSku } from '../api';
import type { Navigate } from '../router';

type SearchApi = Pick<typeof api, 'catalogSkus'>;

export function GlobalSearch({ navigate, client = api }: { navigate: Navigate; client?: SearchApi }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [items, setItems] = useState<CatalogSku[]>([]);
  const [status, setStatus] = useState<'idle' | 'loading' | 'success' | 'error'>('idle');
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  const show = () => {
    previousFocusRef.current = triggerRef.current;
    setOpen(true);
    if (status === 'idle' || status === 'error') {
      setStatus('loading');
      client.catalogSkus().then((page) => {
        setItems(page.items);
        setStatus('success');
      }).catch(() => setStatus('error'));
    }
  };
  const close = () => {
    setOpen(false);
    setQuery('');
    setActiveIndex(0);
    queueMicrotask(() => previousFocusRef.current?.focus());
  };

  useEffect(() => {
    const onShortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        show();
      } else if (open && event.key === 'Escape') {
        event.preventDefault();
        close();
      }
    };
    window.addEventListener('keydown', onShortcut);
    return () => window.removeEventListener('keydown', onShortcut);
  }, [open, status]);

  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  const results = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase('ru');
    if (!normalized) return items.slice(0, 8);
    return items.filter((item) => item.stable_name.toLocaleLowerCase('ru').includes(normalized)).slice(0, 8);
  }, [items, query]);

  const choose = (item: CatalogSku) => {
    navigate(`/market?search=${encodeURIComponent(item.stable_name)}`);
    close();
  };
  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown' && results.length > 0) {
      event.preventDefault();
      setActiveIndex((index) => (index + 1) % results.length);
    } else if (event.key === 'ArrowUp' && results.length > 0) {
      event.preventDefault();
      setActiveIndex((index) => (index - 1 + results.length) % results.length);
    } else if (event.key === 'Enter' && results[activeIndex]) {
      event.preventDefault();
      choose(results[activeIndex]);
    }
  };

  return <>
    <button ref={triggerRef} className="search global-search-trigger" type="button" aria-label="Открыть глобальный поиск" aria-haspopup="dialog" aria-expanded={open} onClick={show}><Search size={16} /><span>Поиск скина</span><kbd>⌘K</kbd></button>
    {open && <div className="global-search-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) close(); }}>
      <section className="global-search-dialog panel" role="dialog" aria-modal="true" aria-label="Глобальный поиск">
        <div className="global-search-input"><Search size={18} /><input ref={inputRef} type="search" role="searchbox" aria-label="Поиск по маркету" value={query} onChange={(event) => { setQuery(event.target.value); setActiveIndex(0); }} onKeyDown={onKeyDown} placeholder="AK-47, AWP, название скина" /><button type="button" onClick={close} aria-label="Закрыть поиск"><X size={17} /></button></div>
        {status === 'loading' && <p className="global-search-state">Загружаем каталог…</p>}
        {status === 'error' && <p className="global-search-state" role="alert">Каталог недоступен. Результаты поиска не подменены.</p>}
        {status === 'success' && <div className="global-search-results" role="listbox" aria-label="Результаты поиска">{results.length ? results.map((item, index) => <button type="button" role="option" aria-selected={index === activeIndex} className={index === activeIndex ? 'active' : ''} key={item.sku_id} onMouseEnter={() => setActiveIndex(index)} onClick={() => choose(item)}><span>{item.weapon || 'CS2'}</span><strong>{item.skin_name || item.stable_name}</strong><small>{item.wear_band.replace(/_/g, ' ')}</small></button>) : <p className="global-search-state">Ничего не найдено.</p>}</div>}
      </section>
    </div>}
  </>;
}
