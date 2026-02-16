import { Send } from 'lucide-react';

interface DmComposerProps {
    value: string;
    friendName: string;
    onChange: (value: string) => void;
    onSend: () => Promise<void>;
}

export function DmComposer({ value, friendName, onChange, onSend }: DmComposerProps) {
    return (
        <div className="p-4 pt-2 flex-shrink-0 border-t border-white/5">
            <div className="relative bg-surface rounded-lg flex items-center p-1 ring-1 ring-white/10 focus-within:ring-primary/50 transition">
                <input
                    type="text"
                    value={value}
                    onChange={(e) => onChange(e.target.value)}
                    onKeyDown={(e) => {
                        if (e.key === 'Enter' && !e.shiftKey) {
                            e.preventDefault();
                            void onSend();
                        }
                    }}
                    placeholder={`Message ${friendName} • Markdown supported`}
                    className="bg-transparent flex-1 px-3 py-2 outline-none text-sm"
                />

                <button
                    onClick={() => void onSend()}
                    className="p-2 text-primary hover:text-primary/80 disabled:opacity-50 disabled:cursor-not-allowed"
                    disabled={!value.trim()}
                >
                    <Send className="w-5 h-5" />
                </button>
            </div>
        </div>
    );
}
