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
import { api, inventoryItemToSkin, isDevelopmentFallbackEnabled, type CatalogSku, type MarketValuation } from './api';
import { items as mockItems, results } from './mocks/dev-data';
import { AppShell, type ApiStatus } from './components/AppShell';
import { SkinCard } from './components/SkinCard';
import { resolveSkinImage, SkinImage } from './components/SkinImage';
import { NotFoundPage } from './pages/NotFoundPage';
import { useRouter, type AppRoute, type Navigate } from './router';
import type { InventoryItem } from './types';

type Item = InventoryItem;
const developmentFallbackEnabled = isDevelopmentFallbackEnabled();

function money(value: number | null) { return value === null ? 'НЕДОСТУПНО' : `${value.toLocaleString('ru-RU')} CC`; }

function ContractsPage({ navigate, setApiStatus }: { navigate: Navigate; setApiStatus: (status: ApiStatus) => void }) {
  const [items, setItems] = useState<Item[]>(developmentFallbackEnabled ? mockItems : []);
  const [selected, setSelected] = useState<Item[]>(developmentFallbackEnabled ? mockItems.slice(0, 4) : []);
  const [usingDevelopmentFallback, setUsingDevelopmentFallback] = useState(developmentFallbackEnabled);
  const [revealOpen, setRevealOpen] = useState(false);
  const [processing, setProcessing] = useState(false);
  const [verificationOpen, setVerificationOpen] = useState(false);
  const [verificationId, setVerificationId] = useState('');
  const [verificationMessage, setVerificationMessage] = useState('');
  const [marketMessage, setMarketMessage] = useState('');
  useEffect(() => {
    setApiStatus('connecting');
    Promise.all([api.inventory(), api.catalogSkus(), api.marketValuations()])
      .then(([inventory, catalog, valuations]) => {
        const catalogBySku = new Map(catalog.items.map((row: CatalogSku) => [row.sku_id, row]));
        const valuationBySku = new Map(valuations.items.map((row: MarketValuation) => [row.sku_id, row]));
        const mapped = inventory.items.map((row) => inventoryItemToSkin(row, catalogBySku.get(row.sku_id), valuationBySku.get(row.sku_id)));
        setApiStatus('live');
        setUsingDevelopmentFallback(false);
        setItems(mapped);
        setSelected(mapped.filter((item) => !item.locked).slice(0, 4));
      })
      .catch(() => {
        setApiStatus('fallback');
        if (developmentFallbackEnabled) {
          setUsingDevelopmentFallback(true);
          setItems(mockItems);
          setSelected(mockItems.slice(0, 4));
        } else {
          setUsingDevelopmentFallback(false);
          setItems([]);
          setSelected([]);
        }
      });
  }, []);
  const total = useMemo(
    () => selected.some((item) => item.price === null)
      ? null
      : selected.reduce((sum, item) => sum + (item.price ?? 0), 0),
    [selected],
  );
  const nextSelectable = items.find((item) => !item.locked && !selected.some((selectedItem) => selectedItem.id === item.id));
  const toggleItem = (item: Item) => {
    if (item.locked) return;
    setSelected((current) => current.some((x) => x.id === item.id) ? current.filter((x) => x.id !== item.id) : current.length < 10 ? [...current, item] : current);
  };
  const commitContract = async () => { if (selected.length < 4 || processing) return; setProcessing(true); setRevealOpen(true); window.setTimeout(() => setProcessing(false), 6400); };
  const verifyContract = async (event: React.FormEvent) => { event.preventDefault(); setVerificationMessage('Проверяем операцию…'); try { const found = await api.verifyContract(verificationId); setVerificationMessage(found ? `Операция найдена в вашей истории · ${found.status}` : 'Операция не найдена или недоступна для этого пользователя.'); } catch { setVerificationMessage('Не удалось загрузить данные проверки. Повторите после входа.'); } };
  const buyMarket = async (item: Item) => { setMarketMessage('Проверяем авторизацию и доступность предмета…'); if (typeof item.skuId !== 'string') { setMarketMessage('Покупка недоступна для демо-данных.'); return; } try { await api.marketPurchase(item.skuId, crypto.randomUUID()); toggleItem(item); setMarketMessage(`${item.weapon} | ${item.skin} добавлен в инвентарь.`); } catch { setMarketMessage('Покупка не выполнена: войдите в профиль или проверьте баланс.'); } };
  return <>
      <section className="intro"><div><span className="eyebrow">КОНТРАКТЫ / WORKSPACE</span><h1>СОЗДАТЬ <em>КОНТРАКТ</em></h1><p>Выберите от 4 до 10 скинов. Соберите контракт и получите один результат.</p></div><div className="trust"><ShieldCheck size={17} /> ПРОЗРАЧНАЯ МЕХАНИКА <span>·</span> CC ECONOMY</div></section>
      <section className="builder-layout"><div className="builder panel"><div className="section-head"><div><h2>ВАШИ ПРЕДМЕТЫ</h2><span>{selected.length} / 10 ПРЕДМЕТОВ</span></div><button className="filter">ВСЕ ПРЕДМЕТЫ <ChevronDown size={15} /></button></div><div className="selection-grid">{selected.map((item) => <SkinCard item={item} selected onRemove={() => toggleItem(item)} key={item.id} />)}{Array.from({ length: Math.max(0, 6 - selected.length) }).map((_, index) => <button className="empty-slot" key={index} disabled={!nextSelectable} onClick={() => nextSelectable && toggleItem(nextSelectable)}><CirclePlus size={19} /><span>ДОБАВИТЬ</span></button>)}</div><div className="builder-note"><span><Sparkles size={15} /> Эти предметы соберутся в один контракт</span><span>Минимум 4 · максимум 10</span></div></div><aside className="summary panel"><span className="eyebrow">CONTRACT / READY</span><h2>КОНТРАКТ</h2><div className="summary-rows"><div><span>СТОИМОСТЬ</span><strong>{money(total)}</strong></div><div><span>ПРЕДМЕТОВ</span><strong>{selected.length} / 10</strong></div><div><span>ВОЗМОЖНЫХ РЕЗУЛЬТАТОВ</span><strong>—</strong></div></div><button className="primary" disabled={selected.length < 4 || processing} onClick={commitContract}>{processing ? "ФИКСИРУЕМ…" : "ЗАКЛЮЧИТЬ КОНТРАК"} <ArrowRight size={17} /></button><small>После подтверждения выбранные предметы будут использованы в контракте.</small></aside></section>
      <section className="content-section"><div className="section-title"><div><span className="eyebrow">OUTPUT RANGE</span><h2>ВОЗМОЖНЫЕ РЕЗУЛЬТАТЫ</h2></div><button className="text-button">ПОКАЗАТЬ ВСЕ <ArrowRight size={15} /></button></div><div className="result-grid">{results.map((item) => <SkinCard item={item} key={item.id} />)}</div></section>
      <section className="content-section inventory"><div className="section-title"><div><span className="eyebrow">YOUR COLLECTION{usingDevelopmentFallback ? ' / DEV DATA' : ''}</span><h2>ВАШ ИНВЕНТАРЬ</h2></div><button className="filter"><Search size={15} /> НАЙТИ ПРЕДМЕТ</button></div><div className="inventory-grid">{items.map((item) => <SkinCard item={item} selected={selected.some((x) => x.id === item.id)} onAdd={() => toggleItem(item)} key={item.id} />)}</div></section>
      <section className="content-section live-contracts"><div className="section-title"><div><span className="eyebrow">LIVE ACTIVITY / NO GAMBLING</span><h2>СЕЙЧАС СОБИРАЮТ</h2></div><span className="quiet">Только подтверждённые операции</span></div><div className="activity-empty"><span className="live-dot" /><div><strong>Пока нет новых контрактов</strong><small>Здесь появятся реальные операции пользователей после подтверждения.</small></div></div></section>
      <section className="content-section market-section"><div className="section-title"><div><span className="eyebrow">MARKET / CC ECONOMY</span><h2>МАРКЕТ</h2><p className="section-subtitle">Выберите предметы для инвентаря и будущих контрактов.</p>{marketMessage && <p className="verification-message">{marketMessage}</p>}</div><button className="text-button" onClick={() => navigate('/market')}>ОТКРЫТЬ МАРКЕТ <ArrowRight size={15} /></button></div><div className="market-table">{items.slice(0, 4).map((item) => <div className="market-row" key={item.id}><SkinCard item={item} /><div className="stock"><span>НАЛИЧИЕ</span><strong>{usingDevelopmentFallback ? 'DEV DATA' : 'НЕДОСТУПНО'}</strong></div><button className="buy-button" onClick={() => buyMarket(item)}>КУПИТЬ</button></div>)}</div></section>
      <section className="content-section history-section"><div className="section-title"><div><span className="eyebrow">AUDIT TRAIL / PUBLIC VIEW</span><h2>ПОСЛЕДНИЕ КОНТРАКТЫ</h2></div><button className="text-button">ИСТОРИЯ <ArrowRight size={15} /></button></div><div className="history-empty"><span>—</span><div><strong>История появится после первого завершённого контракта</strong><small>Здесь будут только user-facing данные операции.</small></div></div></section>
      <section className="content-section transparency"><div className="section-title"><div><span className="eyebrow">TRANSPARENCY / CONTROL</span><h2>ПРОЗРАЧНОСТЬ</h2><p className="section-subtitle">Всё необходимое для понимания ваших операций.</p></div></div><div className="transparency-grid">{[['ПРАВИЛА КОНТРАКТОВ','Как создаётся и подтверждается контракт.'],['ИСТОРИЯ','Ваши завершённые операции.'],['ПРОВЕРКА ОПЕРАЦИИ','Проверка контракта по ID.'],['БЕЗОПАСНОСТЬ','Защита аккаунта и операций.']].map(([title, text]) => <button className="info-card" key={title} onClick={() => title === 'ПРОВЕРКА ОПЕРАЦИИ' && setVerificationOpen(true)}><strong>{title}</strong><span>{text}</span><ArrowRight size={15} /></button>)}</div></section>
      <section className="content-section information"><div className="section-title"><div><span className="eyebrow">SERVICE INFORMATION</span><h2>ВАЖНО</h2></div></div><div className="info-strip"><span><b>18+</b> Сервис предназначен для совершеннолетних пользователей.</span><span><b>CC</b> Предметы используются внутри CONTRACTER.</span><span><b>ОСОЗНАННО</b> Контролируйте свои расходы.</span></div></section>
  {revealOpen && <div className="reveal-backdrop"><div className={`reveal-panel cinematic-reveal ${processing ? 'is-processing' : 'is-result'}`}><div className="reveal-beam" /><div className="reveal-orbit orbit-one" /><div className="reveal-orbit orbit-two" /><span className="eyebrow">CONTRACTER / {processing ? 'LOCK · SIGN · MERGE' : 'RESULT VERIFIED'}</span>{processing ? <><h2>СОБИРАЕМ РЕЗУЛЬТАТ</h2><p className="reveal-caption">Проверяем подпись и объединяем предметы</p><div className="reveal-core"><div className="core-ring" /><div className="core-mark">CC</div></div><div className="reveal-items">{selected.map((item) => <span key={item.id}><SkinImage src={resolveSkinImage(item)} alt="" layout="inline" className="reveal-art" />{item.weapon}<i /></span>)}</div><div className="reveal-progress"><span /></div><p>Не закрывайте окно — операция фиксируется сервером.</p></> : <><span className="result-kicker">CONTRACT COMPLETE</span><h2>ВАШ РЕЗУЛЬТАТ</h2><div className="result-skin"><SkinImage src={resolveSkinImage(results[0])} alt={`${results[0].weapon} | ${results[0].skin}`} layout="inline" className="result-art" /><div className="result-copy"><b>{results[0].weapon}</b><strong>{results[0].skin}</strong><span>{results[0].wear} · {results[0].rarity}</span></div></div><button className="primary" onClick={() => { setRevealOpen(false); setSelected([]); }}>СОБРАТЬ НОВЫЙ <ArrowRight size={16} /></button><button className="close-login" onClick={() => setRevealOpen(false)}>ЗАКРЫТЬ</button></>}</div></div>}
  {verificationOpen && <div className="login-backdrop"><form className="login-modal" onSubmit={verifyContract}><span className="eyebrow">PUBLIC VERIFICATION</span><h2>ПРОВЕРИТЬ КОНТРАК</h2><label>ID ОПЕРАЦИИ<input value={verificationId} onChange={(event) => setVerificationId(event.target.value)} placeholder="CTR-8F4A91..." required /></label>{verificationMessage && <p className="verification-message">{verificationMessage}</p>}<button className="primary" type="submit">ПРОВЕРИТЬ <ShieldCheck size={16} /></button><button className="close-login" type="button" onClick={() => setVerificationOpen(false)}>ОТМЕНА</button></form></div>}
  </>;
}

const routeCopy: Partial<Record<AppRoute['id'], { eyebrow: string; title: string; text: string }>> = {
  market: { eyebrow: 'MARKET / CC ECONOMY', title: 'Маркет', text: 'Каталог предметов и покупка будут доступны на этой странице.' },
  inventory: { eyebrow: 'COLLECTION / INVENTORY', title: 'Инвентарь', text: 'Ваши предметы и фильтры будут доступны на этой странице.' },
  history: { eyebrow: 'AUDIT TRAIL / HISTORY', title: 'История контрактов', text: 'Завершённые операции будут доступны после загрузки данных аккаунта.' },
  profile: { eyebrow: 'ACCOUNT / PROFILE', title: 'Профиль', text: 'Для доступа к данным аккаунта потребуется авторизация.' },
  transparency: { eyebrow: 'TRANSPARENCY / CONTROL', title: 'Прозрачность', text: 'Здесь будут опубликованы правила контрактов и способы проверки операций.' },
  terms: { eyebrow: 'INFORMATION / TERMS', title: 'Условия использования', text: 'Актуальные условия сервиса будут опубликованы на этой странице.' },
  privacy: { eyebrow: 'INFORMATION / PRIVACY', title: 'Политика конфиденциальности', text: 'Актуальная информация об обработке данных будет опубликована на этой странице.' },
  support: { eyebrow: 'SERVICE / SUPPORT', title: 'Поддержка', text: 'Контакты и способы обращения будут опубликованы на этой странице.' },
};

function RoutePage({ route, navigate }: { route: AppRoute; navigate: Navigate }) {
  if (route.id === 'contract-details') {
    return <section className="route-state"><span className="eyebrow">CONTRACT / DETAILS</span><h1>Контракт {route.params.contractId}</h1><p>Данные операции будут загружены из истории контрактов.</p><button className="primary route-state-action" onClick={() => navigate('/history')}>К истории <ArrowRight size={16} /></button></section>;
  }

  const copy = routeCopy[route.id];
  if (!copy) return null;
  return <section className="route-state"><span className="eyebrow">{copy.eyebrow}</span><h1>{copy.title}</h1><p>{copy.text}</p>{route.id === 'profile' && <button className="primary route-state-action" onClick={() => navigate('/login')}>Войти <ArrowRight size={16} /></button>}</section>;
}

function LoginPage({ navigate, setApiStatus }: { navigate: Navigate; setApiStatus: (status: ApiStatus) => void }) {
  const [login, setLogin] = useState('');
  const [password, setPassword] = useState('');
  const [loginError, setLoginError] = useState('');
  const submitLogin = async (event: React.FormEvent) => {
    event.preventDefault();
    setLoginError('');
    try {
      await api.login(login, password);
      setApiStatus('live');
      navigate('/contracts', { replace: true });
    } catch {
      setLoginError('Не удалось войти. Проверьте логин и пароль.');
    }
  };

  return <form className="login-modal route-login" onSubmit={submitLogin}><span className="eyebrow">CONTRACTER / ACCOUNT</span><h1>Войти</h1><label>ЛОГИН<input value={login} onChange={(event) => setLogin(event.target.value)} autoComplete="username" required /></label><label>ПАРОЛЬ<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" required /></label>{loginError && <p className="login-error">{loginError}</p>}<button className="primary" type="submit">ПРОДОЛЖИТЬ <ArrowRight size={16} /></button></form>;
}

export function App() {
  const { route, navigate } = useRouter();
  const [apiStatus, setApiStatus] = useState<ApiStatus>(() => route.id === 'contracts' ? 'connecting' : 'unverified');

  let page;
  if (route.id === 'contracts') page = <ContractsPage navigate={navigate} setApiStatus={setApiStatus} />;
  else if (route.id === 'login') page = <LoginPage navigate={navigate} setApiStatus={setApiStatus} />;
  else if (route.id === 'not-found') page = <NotFoundPage navigate={navigate} />;
  else page = <RoutePage route={route} navigate={navigate} />;

  return <AppShell route={route} navigate={navigate} apiStatus={apiStatus}>{page}</AppShell>;
}

createRoot(document.getElementById('root')!).render(<App />);
