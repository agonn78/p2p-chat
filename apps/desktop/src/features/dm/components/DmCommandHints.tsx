import type { SlashCommandConfig } from '../constants/slashCommands';

interface DmCommandHintsProps {
    commands: Array<[string, SlashCommandConfig]>;
    onSelectCommand: (command: string) => void;
}

export function DmCommandHints({ commands, onSelectCommand }: DmCommandHintsProps) {
    if (commands.length === 0) {
        return null;
    }

    return (
        <div className="mx-4 mb-2 bg-surface rounded-lg border border-white/10 overflow-hidden">
            {commands.map(([command, info]) => (
                <button
                    key={command}
                    onClick={() => onSelectCommand(command)}
                    className="w-full flex items-center justify-between px-4 py-2 hover:bg-white/5 transition text-left"
                >
                    <span className="text-primary font-mono">{command}</span>
                    <span className="text-gray-500 text-sm">{info.description}</span>
                </button>
            ))}
        </div>
    );
}
