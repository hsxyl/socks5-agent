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
      const res = await fetch('http://localhost:3000/api/clients');
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
      const res = await fetch('http://localhost:3000/api/cards');
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
      const res = await fetch('http://localhost:3000/api/logs');
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
      await fetch('http://localhost:3000/api/cards', {
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
        <h1>Edge Proxy - Admin Dashboard</h1>
        <div className="tabs">
          <button 
            className={`tab ${activeTab === 'clients' ? 'active' : ''}`}
            onClick={() => setActiveTab('clients')}
          >
            Edge Clients
          </button>
          <button 
            className={`tab ${activeTab === 'cards' ? 'active' : ''}`}
            onClick={() => setActiveTab('cards')}
          >
            Card Management
          </button>
          <button 
            className={`tab ${activeTab === 'logs' ? 'active' : ''}`}
            onClick={() => setActiveTab('logs')}
          >
            Proxy Logs
          </button>
        </div>
      </header>
      <main>
        {activeTab === 'clients' ? (
          <div className="card">
            <div className="card-header">
              <h2>Edge Clients</h2>
              <button onClick={fetchClients} className="refresh-btn">Refresh</button>
            </div>
            {loadingClients ? (
              <p className="loading">Loading clients...</p>
            ) : (
              <div className="table-responsive">
                <table>
                  <thead>
                    <tr>
                      <th>Device ID</th>
                      <th>IP Address</th>
                      <th>Status</th>
                      <th>Uptime</th>
                      <th>Last Heartbeat</th>
                    </tr>
                  </thead>
                  <tbody>
                    {clients.length === 0 ? (
                      <tr>
                        <td colSpan={5} className="empty">No clients found</td>
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
              <h2>Card Management</h2>
              <div className="generate-form">
                <input 
                  type="number" 
                  value={balanceGb}
                  onChange={(e) => setBalanceGb(Number(e.target.value))}
                  min="1"
                  className="filter-input"
                  title="Balance in GB"
                />
                <span className="unit-label">GB</span>
                <button 
                  onClick={generateCard} 
                  className="generate-btn"
                  disabled={generating}
                >
                  {generating ? 'Generating...' : 'Generate New Card'}
                </button>
              </div>
            </div>
            {loadingCards ? (
              <p className="loading">Loading cards...</p>
            ) : (
              <div className="table-responsive">
                <table>
                  <thead>
                    <tr>
                      <th>Card Key (Username)</th>
                      <th>Remaining Balance</th>
                      <th>Created At</th>
                    </tr>
                  </thead>
                  <tbody>
                    {cards.length === 0 ? (
                      <tr>
                        <td colSpan={3} className="empty">No cards generated yet</td>
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
              <h2>Proxy Logs</h2>
              <button onClick={fetchLogs} className="refresh-btn">Refresh</button>
            </div>
            
            <div className="filter-bar">
              <input 
                type="text" 
                placeholder="Search Target Domain/IP..." 
                className="filter-input"
                value={searchTarget}
                onChange={e => setSearchTarget(e.target.value)}
              />
              <input 
                type="text" 
                placeholder="Search Card Key..." 
                className="filter-input"
                value={searchCard}
                onChange={e => setSearchCard(e.target.value)}
              />
            </div>

            {loadingLogs ? (
              <p className="loading">Loading logs...</p>
            ) : (
              <div className="table-responsive">
                <table>
                  <thead>
                    <tr>
                      <th>ID</th>
                      <th>Card Key</th>
                      <th>Target</th>
                      <th>Device ID</th>
                      <th>Protocol</th>
                      <th className="sortable" onClick={() => toggleSort('bytes_used')}>
                        Traffic Used {sortBy === 'bytes_used' ? (sortOrder === 'asc' ? '↑' : '↓') : ''}
                      </th>
                      <th className="sortable" onClick={() => toggleSort('created_at')}>
                        Time {sortBy === 'created_at' ? (sortOrder === 'asc' ? '↑' : '↓') : ''}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredAndSortedLogs.length === 0 ? (
                      <tr>
                        <td colSpan={7} className="empty">No matching proxy logs found</td>
                      </tr>
                    ) : (
                      filteredAndSortedLogs.map(log => (
                        <tr key={log.id}>
                          <td>{log.id}</td>
                          <td className="card-key" style={{fontSize: '13px'}}>{log.card_key.substring(0, 13)}...</td>
                          <td className="ip">{log.target}</td>
                          <td className="device-id">{log.device_id || 'Fallback'}</td>
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
