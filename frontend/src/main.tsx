import { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { ArrowRight, ChevronDown, CirclePlus, Search, ShieldCheck, Sparkles, X } from 'lucide-react';
import './styles.css';
import './extra.css';
import { api } from './api';

type Item = { id: string; weapon: string; skin: string; wear: string; price: number; color: string; rarity: string };
const items: Item[] = [
  { id: 'm4', weapon: 'M4A1-S', skin: 'Decimator', wear: 'Minimal Wear', price: 120, color: '#59d5e5', rarity: 'Restricted' },
  { id: 'awp', weapon: 'AWP', skin: 'Atheris', wear: 'Field-Tested', price: 230, color: '#8b5cf6', rarity: 'Restricted' },
  { id: 'usp', weapon: 'USP-S', skin: 'Cortex', wear: 'Minimal Wear', price: 85, color: '#d67c9d', rarity: 'Classified' },
  { id: 'ak', weapon: 'AK-47', skin: 'Slate', wear: 'Factory New', price: 190, color: '#e6b35b', rarity: 'Restricted' },
  { id: 'glock', weapon: 'Glock-18', skin: 'Water Elemental', wear: 'Field-Tested', price: 160, color: '#5bb6e9', rarity: 'Classified' },
  { id: 'famas', weapon: 'FAMAS', skin: 'Meow 36', wear: 'Minimal Wear', price: 74, color: '#e57e92', rarity: 'Mil-Spec' },
  { id: 'deagle', weapon: 'Desert Eagle', skin: 'Printstream', wear: 'Field-Tested', price: 490, color: '#d9d9df', rarity: 'Covert' },
  { id: 'mp9', weapon: 'MP9', skin: 'Starlight Protector', wear: 'Minimal Wear', price: 112, color: '#b06be8', rarity: 'Classified' },
];
const results = [items[6], items[1], { ...items[0], id: 'emperor', weapon: 'M4A4', skin: 'The Emperor', price: 2120, color: '#e0a95c', rarity: 'Covert' }, { ...items[2], id: 'neon', weapon: 'AK-47', skin: 'Neon Rider', price: 2680, color: '#ee658a', rarity: 'Covert' }];

function money(value: number) { return `${value.toLocaleString('ru-RU')} CC`; }
function SkinCard({ item, selected, onRemove, onAdd }: { item: Item; selected?: boolean; onRemove?: () => void; onAdd?: () => void }) {
  return <article className={`skin-card ${selected ? 'selected' : ''}`} onClick={onAdd}>
    <div className="skin-art" style={{ '--accent': item.color } as React.CSSProperties}><span>{item.weapon.split('-')[0]}</span><div className="art-line" /></div>
    {selected && <button className="remove" aria-label="Удалить" onClick={(event) => { event.stopPropagation(); onRemove?.(); }}><X size={14} /></button>}
    <div className="skin-meta"><span className="weapon">{item.weapon}</span><strong>{item.skin}</strong><span className="wear">{item.wear}</span></div>
    <div className="card-footer"><span className="rarity" style={{ color: item.color }}>{item.rarity}</span><b>{money(item.price)}</b></div>
  </article>;
}

function App() {
  const [selected, setSelected] = useState<Item[]>(items.slice(0, 4));
  const [tab, setTab] = useState('КОНТРАКТЫ');
  const [apiStatus, setApiStatus] = useState<'connecting' | 'live' | 'fallback'>('connecting');
  useEffect(() => { api.inventory().then(() => setApiStatus('live')).catch(() => setApiStatus('fallback')); }, []);
  const total = useMemo(() => selected.reduce((sum, item) => sum + item.price, 0), [selected]);
  const toggleItem = (item: Item) => setSelected((current) => current.some((x) => x.id === item.id) ? current.filter((x) => x.id !== item.id) : current.length < 10 ? [...current, item] : current);
  return <div className="app-shell">
    <div className="live-bar"><span className="live-dot" /> <b>LIVE</b><span className="live-copy">{apiStatus === 'live' ? 'API CONNECTED · ' : ''}Контракты собираются прямо сейчас</span><span className="live-result">●  void &nbsp; M4A1-S | Decimator &nbsp; <strong>890 CC</strong></span><span className="live-result">●  yng &nbsp; AWP | Neo-Noir &nbsp; <strong>2 340 CC</strong></span></div>
    <header className="header"><div className="brand"><div className="brand-mark"><span>AK</span><i /></div><span>CONTRACTER</span></div><nav>{['КОНТРАКТЫ', 'МАРКЕТ', 'ИНВЕНТАРЬ', 'ИСТОРИЯ'].map((name) => <button className={tab === name ? 'active' : ''} onClick={() => setTab(name)} key={name}>{name}</button>)}</nav><div className="header-actions"><button className="search"><Search size={16} /> Поиск скина</button><span className="balance">{money(1240)}</span><div className="avatar">Y</div></div></header>
    <main>
      <section className="intro"><div><span className="eyebrow">{tab} / WORKSPACE</span><h1>СОЗДАТЬ <em>КОНТРАКТ</em></h1><p>Выберите от 4 до 10 скинов. Соберите контракт и получите один результат.</p></div><div className="trust"><ShieldCheck size={17} /> ПРОЗРАЧНАЯ МЕХАНИКА <span>·</span> CC ECONOMY</div></section>
      <section className="builder-layout"><div className="builder panel"><div className="section-head"><div><h2>ВАШИ ПРЕДМЕТЫ</h2><span>{selected.length} / 10 ПРЕДМЕТОВ</span></div><button className="filter">ВСЕ ПРЕДМЕТЫ <ChevronDown size={15} /></button></div><div className="selection-grid">{selected.map((item) => <SkinCard item={item} selected onRemove={() => toggleItem(item)} key={item.id} />)}{Array.from({ length: Math.max(0, 6 - selected.length) }).map((_, index) => <button className="empty-slot" key={index} onClick={() => toggleItem(items.find((item) => !selected.includes(item)) || items[0])}><CirclePlus size={19} /><span>ДОБАВИТЬ</span></button>)}</div><div className="builder-note"><span><Sparkles size={15} /> Эти предметы соберутся в один контракт</span><span>Минимум 4 · максимум 10</span></div></div><aside className="summary panel"><span className="eyebrow">CONTRACT / READY</span><h2>КОНТРАКТ</h2><div className="summary-rows"><div><span>СТОИМОСТЬ</span><strong>{money(total)}</strong></div><div><span>ПРЕДМЕТОВ</span><strong>{selected.length} / 10</strong></div><div><span>ВОЗМОЖНЫХ РЕЗУЛЬТАТОВ</span><strong>—</strong></div></div><button className="primary" disabled={selected.length < 4}>ЗАКЛЮЧИТЬ КОНТРАК <ArrowRight size={17} /></button><small>После подтверждения выбранные предметы будут использованы в контракте.</small></aside></section>
      <section className="content-section"><div className="section-title"><div><span className="eyebrow">OUTPUT RANGE</span><h2>ВОЗМОЖНЫЕ РЕЗУЛЬТАТЫ</h2></div><button className="text-button">ПОКАЗАТЬ ВСЕ <ArrowRight size={15} /></button></div><div className="result-grid">{results.map((item) => <SkinCard item={item} key={item.id} />)}</div></section>
      <section className="content-section inventory"><div className="section-title"><div><span className="eyebrow">YOUR COLLECTION / 24 ITEMS</span><h2>ВАШ ИНВЕНТАРЬ</h2></div><button className="filter"><Search size={15} /> НАЙТИ ПРЕДМЕТ</button></div><div className="inventory-grid">{items.map((item) => <SkinCard item={item} selected={selected.some((x) => x.id === item.id)} onAdd={() => toggleItem(item)} key={item.id} />)}</div></section>
      <section className="content-section live-contracts"><div className="section-title"><div><span className="eyebrow">LIVE ACTIVITY / NO GAMBLING</span><h2>СЕЙЧАС СОБИРАЮТ</h2></div><span className="quiet">Только подтверждённые операции</span></div><div className="activity-empty"><span className="live-dot" /><div><strong>Пока нет новых контрактов</strong><small>Здесь появятся реальные операции пользователей после подтверждения.</small></div></div></section>
      <section className="content-section market-section"><div className="section-title"><div><span className="eyebrow">MARKET / CC ECONOMY</span><h2>МАРКЕТ</h2><p className="section-subtitle">Выберите предметы для инвентаря и будущих контрактов.</p></div><button className="text-button">ОТКРЫТЬ МАРКЕТ <ArrowRight size={15} /></button></div><div className="market-table">{items.slice(0, 4).map((item) => <div className="market-row" key={item.id}><SkinCard item={item} /><div className="stock"><span>ДОСТУПНО</span><strong>{[12, 7, 18, 4][items.indexOf(item)]}</strong></div><button className="buy-button" onClick={() => toggleItem(item)}>КУПИТЬ</button></div>)}</div></section>
      <section className="content-section history-section"><div className="section-title"><div><span className="eyebrow">AUDIT TRAIL / PUBLIC VIEW</span><h2>ПОСЛЕДНИЕ КОНТРАКТЫ</h2></div><button className="text-button">ИСТОРИЯ <ArrowRight size={15} /></button></div><div className="history-empty"><span>—</span><div><strong>История появится после первого завершённого контракта</strong><small>Здесь будут только user-facing данные операции.</small></div></div></section>
      <section className="content-section transparency"><div className="section-title"><div><span className="eyebrow">TRANSPARENCY / CONTROL</span><h2>ПРОЗРАЧНОСТЬ</h2><p className="section-subtitle">Всё необходимое для понимания ваших операций.</p></div></div><div className="transparency-grid">{[['ПРАВИЛА КОНТРАКТОВ','Как создаётся и подтверждается контракт.'],['ИСТОРИЯ','Ваши завершённые операции.'],['ПРОВЕРКА ОПЕРАЦИИ','Проверка контракта по ID.'],['БЕЗОПАСНОСТЬ','Защита аккаунта и операций.']].map(([title, text]) => <button className="info-card" key={title}><strong>{title}</strong><span>{text}</span><ArrowRight size={15} /></button>)}</div></section>
    </main><footer><span>CONTRACTER © 2026</span><span>Внутренняя валюта: CC · 18+</span><span>TRANSPARENCY / SECURITY / TERMS</span></footer>
  </div>;
}
createRoot(document.getElementById('root')!).render(<App />);
