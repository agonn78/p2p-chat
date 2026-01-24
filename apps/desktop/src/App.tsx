import { useState } from 'react';
import { Mic, Video, Settings, Hash, Send, UserPlus } from 'lucide-react';
// import { invoke } from '@tauri-apps/api/core';

// Mock data
const channels = ['general', 'random', 'development', 'music'];
const messages = [
    { id: 1, user: 'Alice', content: 'Hey everyone! Ready for the P2P call?', time: '10:00' },
    { id: 2, user: 'Bob', content: 'Yes, just setting up my mic.', time: '10:01' },
];

function App() {
    const [activeChannel, setActiveChannel] = useState('general');
    const [msgInput, setMsgInput] = useState('');

    // const joinCall = async () => {
    //   await invoke('join_call', { channel: activeChannel });
    // };

    return (
        <div className="flex h-screen w-full bg-background text-white overflow-hidden font-sans">
            {/* Sidebar */}
            <div className="w-64 bg-surface flex flex-col border-r border-white/5 relative z-10 glass-panel">
                <div className="p-4 border-b border-white/5">
                    <h1 className="text-xl font-bold bg-gradient-to-r from-primary to-secondary bg-clip-text text-transparent">
                        P2P Nitro
                    </h1>
                </div>

                <div className="flex-1 overflow-y-auto p-2 space-y-1">
                    <div className="text-xs font-semibold text-gray-500 uppercase px-2 py-2">Text Channels</div>
                    {channels.map(channel => (
                        <button
                            key={channel}
                            onClick={() => setActiveChannel(channel)}
                            className={`w-full flex items-center px-2 py-2 rounded-md transition config-option ${activeChannel === channel ? 'bg-primary/20 text-primary' : 'hover:bg-white/5 text-gray-400'
                                }`}
                        >
                            <Hash className="w-4 h-4 mr-2" />
                            {channel}
                        </button>
                    ))}
                </div>

                {/* User Status / Controls */}
                <div className="p-3 bg-black/20 backdrop-blur-md border-t border-white/5">
                    <div className="flex items-center justify-between">
                        <div className="flex items-center">
                            <div className="w-8 h-8 rounded-full bg-gradient-to-tr from-primary to-secondary" />
                            <div className="ml-2">
                                <div className="text-sm font-medium">User</div>
                                <div className="text-xs text-green-400">Online</div>
                            </div>
                        </div>
                        <div className="flex space-x-1">
                            <button className="p-1.5 hover:bg-white/10 rounded"><Mic className="w-4 h-4" /></button>
                            <button className="p-1.5 hover:bg-white/10 rounded"><Settings className="w-4 h-4" /></button>
                        </div>
                    </div>
                </div>
            </div>

            {/* Main Chat Area */}
            <div className="flex-1 flex flex-col relative bg-background/50">
                {/* Header */}
                <div className="h-14 border-b border-white/5 flex items-center justify-between px-4 bg-surface/50 backdrop-blur-sm">
                    <div className="flex items-center text-gray-200">
                        <Hash className="w-5 h-5 mr-2 text-gray-500" />
                        <span className="font-semibold">{activeChannel}</span>
                    </div>
                    <div className="flex items-center space-x-3">
                        <button className="flex items-center px-3 py-1.5 bg-green-600 hover:bg-green-500 text-white text-sm rounded-md transition shadow-lg shadow-green-900/20">
                            <Video className="w-4 h-4 mr-2" />
                            Join Call
                        </button>
                        <button className="p-2 hover:bg-white/5 rounded-full text-gray-400"><UserPlus className="w-5 h-5" /></button>
                    </div>
                </div>

                {/* Messages */}
                <div className="flex-1 overflow-y-auto p-4 space-y-4">
                    {messages.map(msg => (
                        <div key={msg.id} className="flex group hover:bg-white/[0.02] -mx-4 px-4 py-1 transition">
                            <div className="w-10 h-10 rounded-full bg-gray-700 mt-1 flex-shrink-0" />
                            <div className="ml-3">
                                <div className="flex items-baseline">
                                    <span className="font-medium text-gray-200 hover:underline cursor-pointer">{msg.user}</span>
                                    <span className="ml-2 text-xs text-gray-500">{msg.time}</span>
                                </div>
                                <p className="text-gray-300 leading-relaxed">{msg.content}</p>
                            </div>
                        </div>
                    ))}
                </div>

                {/* Input */}
                <div className="p-4 pt-2">
                    <div className="relative bg-surface rounded-lg flex items-center p-1 ring-1 ring-white/10 focus-within:ring-primary/50 transition">
                        <button className="p-2 text-gray-400 hover:text-gray-200"><UserPlus className="w-5 h-5" /></button>
                        <input
                            type="text"
                            value={msgInput}
                            onChange={(e) => setMsgInput(e.target.value)}
                            placeholder={`Message #${activeChannel}`}
                            className="bg-transparent flex-1 px-2 py-2 outline-none text-sm"
                        />
                        <button className="p-2 text-primary hover:text-primary/80"><Send className="w-5 h-5" /></button>
                    </div>
                </div>

                {/* Media Overlay (Glassmorphism) -- Only visible when in call */}
                <div className="absolute top-20 right-4 w-64 bg-black/60 backdrop-blur-xl border border-white/10 rounded-xl p-4 shadow-2xl z-20 hidden">
                    <h3 className="text-xs font-bold text-gray-400 uppercase tracking-widest mb-3">Call Stats</h3>
                    <div className="space-y-2">
                        <div className="flex justify-between text-xs">
                            <span className="text-gray-500">Ping</span>
                            <span className="text-green-400">24ms</span>
                        </div>
                        <div className="flex justify-between text-xs">
                            <span className="text-gray-500">Loss</span>
                            <span className="text-blue-400">0%</span>
                        </div>

                        {/* VU Meter */}
                        <div className="mt-2">
                            <div className="flex justify-between text-[10px] text-gray-500 mb-1">
                                <span>MIC</span>
                                <span>-12dB</span>
                            </div>
                            <div className="w-full h-1 bg-gray-700 rounded-full overflow-hidden">
                                <div className="h-full bg-gradient-to-r from-green-500 to-red-500 w-[70%]" />
                            </div>
                        </div>
                    </div>
                </div>

            </div>
        </div>
    );
}

export default App;
