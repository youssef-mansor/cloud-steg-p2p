import { useState } from 'react';
import { ImageEncryptionTab } from './components/ImageEncryptionTab';
import { ClusterMonitoringTab } from './components/ClusterMonitoringTab';
import { Cog, Image, Activity } from 'lucide-react';

function App() {
  const [activeTab, setActiveTab] = useState<'encryption' | 'monitoring'>('encryption');

  return (
    <div className="min-h-screen bg-gray-100">
      {/* Header */}
      <header className="bg-gradient-to-r from-blue-600 to-blue-800 text-white shadow-lg">
        <div className="container mx-auto px-4 py-6">
          <div className="flex items-center gap-3">
            <Cog size={36} className="animate-spin-slow" />
            <div>
              <h1 className="text-3xl font-bold">Raft Cluster Client</h1>
              <p className="text-blue-100 text-sm">
                Image Encryption & Load Testing Dashboard
              </p>
            </div>
          </div>
        </div>
      </header>

      {/* Tab Navigation */}
      <nav className="bg-white border-b shadow-sm">
        <div className="container mx-auto px-4">
          <div className="flex gap-1">
            <button
              onClick={() => setActiveTab('encryption')}
              className={`flex items-center gap-2 px-6 py-4 font-semibold border-b-2 transition-colors ${
                activeTab === 'encryption'
                  ? 'border-blue-600 text-blue-600'
                  : 'border-transparent text-gray-600 hover:text-gray-900'
              }`}
            >
              <Image size={20} />
              Image Encryption & Testing
            </button>
            <button
              onClick={() => setActiveTab('monitoring')}
              className={`flex items-center gap-2 px-6 py-4 font-semibold border-b-2 transition-colors ${
                activeTab === 'monitoring'
                  ? 'border-blue-600 text-blue-600'
                  : 'border-transparent text-gray-600 hover:text-gray-900'
              }`}
            >
              <Activity size={20} />
              Cluster Monitoring
            </button>
          </div>
        </div>
      </nav>

      {/* Main Content */}
      <main className="container mx-auto px-4 py-8">
        {activeTab === 'encryption' && <ImageEncryptionTab />}
        {activeTab === 'monitoring' && <ClusterMonitoringTab />}
      </main>

      {/* Footer */}
      <footer className="bg-gray-800 text-white mt-12">
        <div className="container mx-auto px-4 py-6">
          <div className="flex flex-col md:flex-row justify-between items-center gap-4">
            <p className="text-sm text-gray-400">
              Raft OpenRaft Demo Client • Built with React + TypeScript + Vite
            </p>
            <div className="flex gap-4 text-sm text-gray-400">
              <span>Default Nodes: 10.40.49.211:8001, 10.40.41.162:8002, 10.40.46.168:8003</span>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}

export default App;
