import { UserCircle } from 'lucide-react';
import type { User } from '../../../types';

interface DmNoSelectionStateProps {
    user: User | null;
}

export function DmNoSelectionState({ user }: DmNoSelectionStateProps) {
    return (
        <div className="flex-1 flex items-center justify-center">
            <div className="text-center text-gray-500">
                <div className="w-32 h-32 rounded-full bg-gradient-to-br from-primary/20 to-secondary/20 flex items-center justify-center mx-auto mb-6">
                    <UserCircle className="w-16 h-16 text-primary/50" />
                </div>
                <p className="text-xl font-medium mb-2">Welcome back, {user?.username}!</p>
                <p>Select a friend to start chatting</p>
            </div>
        </div>
    );
}
