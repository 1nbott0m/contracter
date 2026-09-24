import { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { ArrowRight, ChevronDown, CirclePlus, Search, ShieldCheck, Sparkles } from 'lucide-react';
import './styles.css';
import './extra.css';
import './login.css';
import './ux.css';
import './reveal-cinematic.css';
import './visual-polish.css';
import './image-states.css';
import './accessibility.css';
import { api, inventoryItemToSkin, type CatalogSku } from './api';
import { items as mockItems, results, type MockItem } from './mocks/dev-data';
import { SkinCard } from './components/SkinCard';
import { Logo } from './components/Logo';
import { resolveSkinImage, SkinImage } from './components/SkinImage';

type Item = MockItem;

function money(value: number) { return `${value.toLocaleString('ru-RU')} CC`; }

function App() {
  const [items, setItems] = useState<Item[]>(mockItems);
  const [selected, setSelected] = useState<Item[]>(mockItems.slice(0, 4));
  const routeTabs: Record<string, string> = { '/': 'КОНТРАКТЫ', '/contracts': 'КОНТРАКТЫ', '/market': 'МАРКЕТ', '/inventory': 'ИНВЕНТАРЬ', '/history': 'ИСТОРИЯ' };
  const [tab, setTab] = useState(() => routeTabs[window.location.pathname] || 'КОНТРАКТЫ');
  const [apiStatus, setApiStatus] = useState<'connecting' | 'live' | 'fallback'>('connecting');
  const [loginOpen, setLoginOpen] = useState(() => window.location.pathname === '/profile');
  const [login, setLogin] = useState('');
  const [password, setPassword] = useState('');
  const [loginError, setLoginError] = useState('');
  const [revealOpen, setRevealOpen] = useState(false);
  const [processing, setProcessing] = useState(false);
  const [verificationOpen, setVerificationOpen] = useState(false);
  const [verificationId, setVerificationId] = useState('');
  const [verificationMessage, setVerificationMessage] = useState('');
  const [infoPage, setInfoPage] = useState<string | null>(null);
  const [marketMessage, setMarketMessage] = useState('');
  const navigate = (next: string) => { const paths: Record<string, string> = { КОНТРАКТЫ: '/contracts', МАРКЕТ: '/market', ИНВЕНТАРЬ: '/inventory', ИСТОРИЯ: '/history' }; setTab(next); window.history.pushState({}, '', paths[next] || '/'); };
  useEffect(() => { const onPopState = () => { setTab(routeTabs[window.location.pathname] || 'КОНТРАКТЫ'); setLoginOpen(window.location.pathname === '/profile'); }; window.addEventListener('popstate', onPopState); return () => window.removeEventListener('popstate', onPopState); }, []);
  useEffect(() => {
    Promise.all([api.inventory(), api.catalogSkus()])
      .then(([inventory, catalog]) => {
        const artworkBySku = new Map(catalog.items.map((row: CatalogSku) => [row.sku_id, row.canonical_image_url]));
        const mapped = inventory.items.map((row) => inventoryItemToSkin(row, artworkBySku.get(row.sku_id)));
        setApiStatus('live');
        if (mapped.length > 0) {
          setItems(mapped);
          setSelected(mapped.filter((item) => !item.locked).slice(0, 4));
        }
      })
      .catch(() => setApiStatus('fallback'));
  }, []);
  const total = useMemo(() => selected.reduce((sum, item) => sum + item.price, 0), [selected]);
  const toggleItem = (item: Item) => setSelected((current) => current.some((x) => x.id === item.id) ? current.filter((x) => x.id !== item.id) : current.length < 10 ? [...current, item] : current);
  const submitLogin = async (event: React.FormEvent) => { event.preventDefault(); setLoginError(''); try { await api.login(login, password); window.history.pushState({}, '', '/contracts'); setLoginOpen(false); setApiStatus('live'); } catch { setLoginError('Не удалось войти. Проверьте логин и пароль.'); } };
  const commitContract = async () => { if (selected.length < 4 || processing) return; setProcessing(true); setRevealOpen(true); window.setTimeout(() => setProcessing(false), 6400); };
  const verifyContract = async (event: React.FormEvent) => { event.preventDefault(); setVerificationMessage('Проверяем операцию…'); try { const history = await api.history(); const found = history.items.find((item) => item.contract_id.toLowerCase() === verificationId.trim().toLowerCase()); setVerificationMessage(found ? `Операция подтверждена · ${found.status} · ${found.input_count} предмета` : 'Операция не найдена или недоступна для этого пользователя.'); } catch { setVerificationMessage('Не удалось загрузить данные проверки. Повторите после входа.'); } };
  const buyMarket = async (item: Item) => { setMarketMessage('Проверяем авторизацию и доступность предмета…'); try { await api.purchase(item.id, crypto.randomUUID()); toggleItem(item); setMarketMessage(`${item.weapon} | ${item.skin} добавлен в инвентарь.`); } catch { setMarketMessage('Покупка не выполнена: войдите в профиль или проверьте баланс.'); } };
  return <div className={`app-shell page-${tab.toLowerCase().replace(/[^a-zа-яё]+/gi, '-')}`}>
    <div className="live-bar"><span className="live-dot" /> <b>LIVE</b><span className="live-copy">{apiStatus === 'live' ? 'API CONNECTED · ' : ''}LIVE ACTIVITY</span><span className="live-count">0 ONLINE</span><span className="live-empty">Пока нет новых контрактов</span></div>
    <header className="header"><Logo /><nav>{['КОНТРАКТЫ', 'МАРКЕТ', 'ИНВЕНТАРЬ', 'ИСТОРИЯ'].map((name) => <button className={tab === name ? 'active' : ''} onClick={() => navigate(name)} key={name}>{name}</button>)}</nav><div className="header-actions"><button className="search"><Search size={16} /> Поиск скина</button><span className="balance">{money(1240)}</span><button className="profile-button" onClick={() => { window.history.pushState({}, '', '/profile'); setLoginOpen(true); }} aria-label="Открыть профиль"><div className="avatar">Y</div><span>ПРОФИЛЬ</span></button></div></header><main>
      <section className="intro"><div><span className="eyebrow">{tab} / WORKSPACE</span><h1>СОЗДАТЬ <em>КОНТРАКТ</em></h1><p>Выберите от 4 до 10 скинов. Соберите контракт и получите один результат.</p></div><div className="trust"><ShieldCheck size={17} /> ПРОЗРАЧНАЯ МЕХАНИКА <span>·</span> CC ECONOMY</div></section>
      <section className="builder-layout"><div className="builder panel"><div className="section-head"><div><h2>ВАШИ ПРЕДМЕТЫ</h2><span>{selected.length} / 10 ПРЕДМЕТОВ</span></div><button className="filter">ВСЕ ПРЕДМЕТЫ <ChevronDown size={15} /></button></div><div className="selection-grid">{selected.map((item) => <SkinCard item={item} selected onRemove={() => toggleItem(item)} key={item.id} />)}{Array.from({ length: Math.max(0, 6 - selected.length) }).map((_, index) => <button className="empty-slot" key={index} onClick={() => toggleItem(items.find((item) => !selected.includes(item)) || items[0])}><CirclePlus size={19} /><span>ДОБАВИТЬ</span></button>)}</div><div className="builder-note"><span><Sparkles size={15} /> Эти предметы соберутся в один контракт</span><span>Минимум 4 · максимум 10</span></div></div><aside className="summary panel"><span className="eyebrow">CONTRACT / READY</span><h2>КОНТРАКТ</h2><div className="summary-rows"><div><span>СТОИМОСТЬ</span><strong>{money(total)}</strong></div><div><span>ПРЕДМЕТОВ</span><strong>{selected.length} / 10</strong></div><div><span>ВОЗМОЖНЫХ РЕЗУЛЬТАТОВ</span><strong>—</strong></div></div><button className="primary" disabled={selected.length < 4 || processing} onClick={commitContract}>{processing ? "ФИКСИРУЕМ…" : "ЗАКЛЮЧИТЬ КОНТРАК"} <ArrowRight size={17} /></button><small>После подтверждения выбранные предметы будут использованы в контракте.</small></aside></section>
      <section className="content-section"><div className="section-title"><div><span className="eyebrow">OUTPUT RANGE</span><h2>ВОЗМОЖНЫЕ РЕЗУЛЬТАТЫ</h2></div><button className="text-button">ПОКАЗАТЬ ВСЕ <ArrowRight size={15} /></button></div><div className="result-grid">{results.map((item) => <SkinCard item={item} key={item.id} />)}</div></section>
      <section className="content-section inventory"><div className="section-title"><div><span className="eyebrow">YOUR COLLECTION / 24 ITEMS</span><h2>ВАШ ИНВЕНТАРЬ</h2></div><button className="filter"><Search size={15} /> НАЙТИ ПРЕДМЕТ</button></div><div className="inventory-grid">{items.map((item) => <SkinCard item={item} selected={selected.some((x) => x.id === item.id)} onAdd={() => toggleItem(item)} key={item.id} />)}</div></section>
      <section className="content-section live-contracts"><div className="section-title"><div><span className="eyebrow">LIVE ACTIVITY / NO GAMBLING</span><h2>СЕЙЧАС СОБИРАЮТ</h2></div><span className="quiet">Только подтверждённые операции</span></div><div className="activity-empty"><span className="live-dot" /><div><strong>Пока нет новых контрактов</strong><small>Здесь появятся реальные операции пользователей после подтверждения.</small></div></div></section>
      <section className="content-section market-section"><div className="section-title"><div><span className="eyebrow">MARKET / CC ECONOMY</span><h2>МАРКЕТ</h2><p className="section-subtitle">Выберите предметы для инвентаря и будущих контрактов.</p>{marketMessage && <p className="verification-message">{marketMessage}</p>}</div><button className="text-button" onClick={() => navigate('МАРКЕТ')}>ОТКРЫТЬ МАРКЕТ <ArrowRight size={15} /></button></div><div className="market-table">{items.slice(0, 4).map((item) => <div className="market-row" key={item.id}><SkinCard item={item} /><div className="stock"><span>ДОСТУПНО</span><strong>{[12, 7, 18, 4][items.indexOf(item)]}</strong></div><button className="buy-button" onClick={() => buyMarket(item)}>КУПИТЬ</button></div>)}</div></section>
      <section className="content-section history-section"><div className="section-title"><div><span className="eyebrow">AUDIT TRAIL / PUBLIC VIEW</span><h2>ПОСЛЕДНИЕ КОНТРАКТЫ</h2></div><button className="text-button">ИСТОРИЯ <ArrowRight size={15} /></button></div><div className="history-empty"><span>—</span><div><strong>История появится после первого завершённого контракта</strong><small>Здесь будут только user-facing данные операции.</small></div></div></section>
      <section className="content-section transparency"><div className="section-title"><div><span className="eyebrow">TRANSPARENCY / CONTROL</span><h2>ПРОЗРАЧНОСТЬ</h2><p className="section-subtitle">Всё необходимое для понимания ваших операций.</p></div></div><div className="transparency-grid">{[['ПРАВИЛА КОНТРАКТОВ','Как создаётся и подтверждается контракт.'],['ИСТОРИЯ','Ваши завершённые операции.'],['ПРОВЕРКА ОПЕРАЦИИ','Проверка контракта по ID.'],['БЕЗОПАСНОСТЬ','Защита аккаунта и операций.']].map(([title, text]) => <button className="info-card" key={title} onClick={() => title === 'ПРОВЕРКА ОПЕРАЦИИ' && setVerificationOpen(true)}><strong>{title}</strong><span>{text}</span><ArrowRight size={15} /></button>)}</div></section>
      <section className="content-section information"><div className="section-title"><div><span className="eyebrow">SERVICE INFORMATION</span><h2>ВАЖНО</h2></div></div><div className="info-strip"><span><b>18+</b> Сервис предназначен для совершеннолетних пользователей.</span><span><b>CC</b> Предметы используются внутри CONTRACTER.</span><span><b>ОСОЗНАННО</b> Контролируйте свои расходы.</span></div></section>
    </main><footer><div><strong>CONTRACTER</strong><span><button className="footer-link" onClick={() => navigate('КОНТРАКТЫ')}>Контракты</button> · <button className="footer-link" onClick={() => navigate('МАРКЕТ')}>Маркет</button> · <button className="footer-link" onClick={() => navigate('ИНВЕНТАРЬ')}>Инвентарь</button> · <button className="footer-link" onClick={() => navigate('ИСТОРИЯ')}>История</button></span></div><div><strong>ИНФОРМАЦИЯ</strong><span><button className="footer-link" onClick={() => setInfoPage('Прозрачность')}>Прозрачность</button> · <button className="footer-link" onClick={() => setInfoPage('Правила контрактов')}>Правила</button> · <button className="footer-link" onClick={() => setVerificationOpen(true)}>Проверка операции</button></span></div><div><strong>18+</strong><span><button className="footer-link" onClick={() => setInfoPage('Условия использования')}>Условия</button> · <button className="footer-link" onClick={() => setInfoPage('Политика конфиденциальности')}>Privacy</button> · <button className="footer-link" onClick={() => setInfoPage('Поддержка')}>Поддержка</button><br />Независимый сервис, не аффилированный с Valve Corporation или Steam.</span></div></footer>
  {revealOpen && <div className="reveal-backdrop"><div className={`reveal-panel cinematic-reveal ${processing ? 'is-processing' : 'is-result'}`}><div className="reveal-beam" /><div className="reveal-orbit orbit-one" /><div className="reveal-orbit orbit-two" /><span className="eyebrow">CONTRACTER / {processing ? 'LOCK · SIGN · MERGE' : 'RESULT VERIFIED'}</span>{processing ? <><h2>СОБИРАЕМ РЕЗУЛЬТАТ</h2><p className="reveal-caption">Проверяем подпись и объединяем предметы</p><div className="reveal-core"><div className="core-ring" /><div className="core-mark">CC</div></div><div className="reveal-items">{selected.map((item) => <span key={item.id}><SkinImage src={resolveSkinImage(item)} alt="" layout="inline" className="reveal-art" />{item.weapon}<i /></span>)}</div><div className="reveal-progress"><span /></div><p>Не закрывайте окно — операция фиксируется сервером.</p></> : <><span className="result-kicker">CONTRACT COMPLETE</span><h2>ВАШ РЕЗУЛЬТАТ</h2><div className="result-skin"><SkinImage src={resolveSkinImage(results[0])} alt={`${results[0].weapon} | ${results[0].skin}`} layout="inline" className="result-art" /><div className="result-copy"><b>{results[0].weapon}</b><strong>{results[0].skin}</strong><span>{results[0].wear} · {results[0].rarity}</span></div></div><button className="primary" onClick={() => { setRevealOpen(false); setSelected([]); }}>СОБРАТЬ НОВЫЙ <ArrowRight size={16} /></button><button className="close-login" onClick={() => setRevealOpen(false)}>ЗАКРЫТЬ</button></>}</div></div>}
  {verificationOpen && <div className="login-backdrop"><form className="login-modal" onSubmit={verifyContract}><span className="eyebrow">PUBLIC VERIFICATION</span><h2>ПРОВЕРИТЬ КОНТРАК</h2><label>ID ОПЕРАЦИИ<input value={verificationId} onChange={(event) => setVerificationId(event.target.value)} placeholder="CTR-8F4A91..." required /></label>{verificationMessage && <p className="verification-message">{verificationMessage}</p>}<button className="primary" type="submit">ПРОВЕРИТЬ <ShieldCheck size={16} /></button><button className="close-login" type="button" onClick={() => setVerificationOpen(false)}>ОТМЕНА</button></form></div>}
  {infoPage && <div className="login-backdrop"><section className="login-modal info-modal"><span className="eyebrow">CONTRACTER / INFORMATION</span><h2>{infoPage}</h2><p>Раздел подготовлен для пользовательской информации CONTRACTER. Здесь будут опубликованы актуальные правила и контакты без раскрытия внутренней серверной логики.</p><button className="primary" onClick={() => setInfoPage(null)}>ПОНЯТНО <ArrowRight size={16} /></button></section></div>}
  {loginOpen && <div className="login-backdrop"><form className="login-modal" onSubmit={submitLogin}><span className="eyebrow">CONTRACTER / ACCOUNT</span><h2>ВОЙТИ</h2><label>ЛОГИН<input value={login} onChange={(event) => setLogin(event.target.value)} autoComplete="username" required /></label><label>ПАРОЛЬ<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" required /></label>{loginError && <p className="login-error">{loginError}</p>}<button className="primary" type="submit">ПРОДОЛЖИТЬ <ArrowRight size={16} /></button><button className="close-login" type="button" onClick={() => setLoginOpen(false)}>ОТМЕНА</button></form></div>}
  </div>;
}
createRoot(document.getElementById('root')!).render(<App />);
