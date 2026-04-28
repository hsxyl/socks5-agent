import { useEffect, useState, useMemo } from 'react';
import './App.css';

interface ClientInfo {
  device_id: string;
  ip_address: string;
  status: string;
  connected_at: string | null;
  last_heartbeat: string | null;
  created_at: string;
}

interface CardInfo {
  card_key: string;
  balance: number;
  created_at: string;
}

interface ProxyLog {
  id: number;
  card_key: string;
  target: string;
  device_id: string | null;
  protocol: string;
  bytes_used: number;
  created_at: string;
}

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:3000';

function App() {
  const [activeTab, setActiveTab] = useState<'clients' | 'cards' | 'logs'>('clients');

  // Clients State
  const [clients, setClients] = useState<ClientInfo[]>([]);
  const [loadingClients, setLoadingClients] = useState(true);

  // Cards State
  const [cards, setCards] = useState<CardInfo[]>([]);
  const [loadingCards, setLoadingCards] = useState(true);
  const [generating, setGenerating] = useState(false);
  const [balanceGb, setBalanceGb] = useState<number>(10);

  // Logs State
  const [logs, setLogs] = useState<ProxyLog[]>([]);
  const [loadingLogs, setLoadingLogs] = useState(true);
  
  // Logs Filtering & Sorting
  const [searchTarget, setSearchTarget] = useState('');
  const [searchCard, setSearchCard] = useState('');
  const [sortBy, setSortBy] = useState<'created_at' | 'bytes_used'>('created_at');
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc');

  const fetchClients = async () => {
    try {
      const res = await fetch(`${API_BASE_URL}/api/clients`);
      const data = await res.json();
      setClients(data);
    } catch (e) {
      console.error(e);
    } finally {
      setLoadingClients(false);
    }
  };

  const fetchCards = async () => {
    try {
      const res = await fetch(`${API_BASE_URL}/api/cards`);
      const data = await res.json();
      setCards(data);
    } catch (e) {
      console.error(e);
    } finally {
      setLoadingCards(false);
    }
  };

  const fetchLogs = async () => {
    try {
      const res = await fetch(`${API_BASE_URL}/api/logs`);
      const data = await res.json();
      setLogs(data);
    } catch (e) {
      console.error(e);
    } finally {
      setLoadingLogs(false);
    }
  };

  const generateCard = async () => {
    if (balanceGb <= 0) return;
    setGenerating(true);
    try {
      await fetch(`${API_BASE_URL}/api/cards`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({ balance_gb: balanceGb })
      });
      await fetchCards();
    } catch (e) {
      console.error(e);
    } finally {
      setGenerating(false);
    }
  };

  useEffect(() => {
    if (activeTab === 'clients') {
      fetchClients();
      const interval = setInterval(fetchClients, 5000);
      return () => clearInterval(interval);
    } else if (activeTab === 'cards') {
      fetchCards();
    } else if (activeTab === 'logs') {
      fetchLogs();
    }
  }, [activeTab]);

  const calculateUptime = (connected_at: string | null, status: string) => {
    if (!connected_at || status !== 'online') return '-';
    const start = new Date(connected_at + 'Z').getTime(); 
    const now = new Date().getTime();
    const diff = Math.floor((now - start) / 1000);
    if (diff < 0) return '0s';
    
    const h = Math.floor(diff / 3600);
    const m = Math.floor((diff % 3600) / 60);
    const s = diff % 60;
    
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  };

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  };

  const toggleSort = (field: 'created_at' | 'bytes_used') => {
    if (sortBy === field) {
      setSortOrder(sortOrder === 'asc' ? 'desc' : 'asc');
    } else {
      setSortBy(field);
      setSortOrder('desc');
    }
  };

  const filteredAndSortedLogs = useMemo(() => {
    let result = [...logs];
    
    if (searchTarget) {
      const lowerTarget = searchTarget.toLowerCase();
      result = result.filter(log => log.target.toLowerCase().includes(lowerTarget));
    }
    
    if (searchCard) {
      const lowerCard = searchCard.toLowerCase();
      result = result.filter(log => log.card_key.toLowerCase().includes(lowerCard));
    }

    result.sort((a, b) => {
      let comparison = 0;
      if (sortBy === 'created_at') {
        comparison = new Date(a.created_at + 'Z').getTime() - new Date(b.created_at + 'Z').getTime();
      } else if (sortBy === 'bytes_used') {
        comparison = a.bytes_used - b.bytes_used;
      }
      return sortOrder === 'asc' ? comparison : -comparison;
    });

    return result;
  }, [logs, searchTarget, searchCard, sortBy, sortOrder]);

  return (
    <div className="container">
      <header>
        <h1>边缘代理控制台 (Edge Proxy Admin)</h1>
        <div className="tabs">
          <button 
            className={`tab ${activeTab === 'clients' ? 'active' : ''}`}
            onClick={() => setActiveTab('clients')}
          >
            边缘节点管理
          </button>
          <button 
            className={`tab ${activeTab === 'cards' ? 'active' : ''}`}
            onClick={() => setActiveTab('cards')}
          >
            计费卡密管理
          </button>
          <button 
            className={`tab ${activeTab === 'logs' ? 'active' : ''}`}
            onClick={() => setActiveTab('logs')}
          >
            代理请求日志
          </button>
        </div>
      </header>
      <main>
        {activeTab === 'clients' ? (
          <div className="card">
            <div className="card-header">
              <h2>边缘节点列表</h2>
              <button onClick={fetchClients} className="refresh-btn">刷新</button>
            </div>
            {loadingClients ? (
              <p className="loading">正在加载边缘节点...</p>
            ) : (
              <div className="table-responsive">
                <table>
                  <thead>
                    <tr>
                      <th>设备标识 (Device ID)</th>
                      <th>公网 IP</th>
                      <th>状态</th>
                      <th>在线时长</th>
                      <th>最后心跳时间</th>
                    </tr>
                  </thead>
                  <tbody>
                    {clients.length === 0 ? (
                      <tr>
                        <td colSpan={5} className="empty">暂无在线边缘设备</td>
                      </tr>
                    ) : (
                      clients.map(client => (
                        <tr key={client.device_id}>
                          <td className="device-id">{client.device_id}</td>
                          <td className="ip">{client.ip_address}</td>
                          <td>
                            <span className={`status-badge ${client.status}`}>
                              {client.status}
                            </span>
                          </td>
                          <td>{calculateUptime(client.connected_at, client.status)}</td>
                          <td className="time">{client.last_heartbeat ? new Date(client.last_heartbeat + 'Z').toLocaleString() : '-'}</td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        ) : activeTab === 'cards' ? (
          <div className="card">
            <div className="card-header">
              <h2>计费卡密管理</h2>
              <div className="generate-form">
                <input 
                  type="number" 
                  value={balanceGb}
                  onChange={(e) => setBalanceGb(Number(e.target.value))}
                  min="1"
                  className="filter-input"
                  title="生成额度(GB)"
                />
                <span className="unit-label">GB</span>
                <button 
                  onClick={generateCard} 
                  className="generate-btn"
                  disabled={generating}
                >
                  {generating ? '生成中...' : '生成新卡密'}
                </button>
              </div>
            </div>
            {loadingCards ? (
              <p className="loading">正在加载卡密数据...</p>
            ) : (
              <div className="table-responsive">
                <table>
                  <thead>
                    <tr>
                      <th>卡密 (代理用户名)</th>
                      <th>剩余可用流量</th>
                      <th>创建时间</th>
                    </tr>
                  </thead>
                  <tbody>
                    {cards.length === 0 ? (
                      <tr>
                        <td colSpan={3} className="empty">暂无已生成的卡密</td>
                      </tr>
                    ) : (
                      cards.map(card => (
                        <tr key={card.card_key}>
                          <td className="card-key">{card.card_key}</td>
                          <td className="balance">{formatBytes(card.balance)}</td>
                          <td className="time">{new Date(card.created_at + 'Z').toLocaleString()}</td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        ) : (
          <div className="card">
            <div className="card-header">
              <h2>代理请求日志</h2>
              <button onClick={fetchLogs} className="refresh-btn">刷新</button>
            </div>
            
            <div className="filter-bar">
              <input 
                type="text" 
                placeholder="搜索目标域名或 IP..." 
                className="filter-input"
                value={searchTarget}
                onChange={e => setSearchTarget(e.target.value)}
              />
              <input 
                type="text" 
                placeholder="搜索卡密..." 
                className="filter-input"
                value={searchCard}
                onChange={e => setSearchCard(e.target.value)}
              />
            </div>

            {loadingLogs ? (
              <p className="loading">正在加载日志...</p>
            ) : (
              <div className="table-responsive">
                <table>
                  <thead>
                    <tr>
                      <th>ID</th>
                      <th>卡密</th>
                      <th>目标地址</th>
                      <th>承载节点 (Device ID)</th>
                      <th>代理协议</th>
                      <th className="sortable" onClick={() => toggleSort('bytes_used')}>
                        消耗流量 {sortBy === 'bytes_used' ? (sortOrder === 'asc' ? '↑' : '↓') : ''}
                      </th>
                      <th className="sortable" onClick={() => toggleSort('created_at')}>
                        请求时间 {sortBy === 'created_at' ? (sortOrder === 'asc' ? '↑' : '↓') : ''}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredAndSortedLogs.length === 0 ? (
                      <tr>
                        <td colSpan={7} className="empty">未找到相关的代理请求日志</td>
                      </tr>
                    ) : (
                      filteredAndSortedLogs.map(log => (
                        <tr key={log.id}>
                          <td>{log.id}</td>
                          <td className="card-key" style={{fontSize: '13px'}}>{log.card_key.substring(0, 13)}...</td>
                          <td className="ip">{log.target}</td>
                          <td className="device-id">{log.device_id || '内置备用IP (Fallback)'}</td>
                          <td>
                            <span className={`status-badge ${log.protocol === 'SOCKS5' ? 'socks5' : 'http'}`}>
                              {log.protocol}
                            </span>
                          </td>
                          <td className="balance" style={{color: '#64748b'}}>{formatBytes(log.bytes_used)}</td>
                          <td className="time">{new Date(log.created_at + 'Z').toLocaleString()}</td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
