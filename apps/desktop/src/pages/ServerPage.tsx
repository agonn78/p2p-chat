import { Hash } from 'lucide-react';
import { useAppStore } from '../store';
import { ChannelList } from '../components/ChannelList';
import { MemberList } from '../components/MemberList';
import { ServerChatView } from '../features/server/components/ServerChatView';

export function ServerPage() {
    const activeChannel = useAppStore((s) => s.activeChannel);

    return (
        <>
            <ChannelList />

            <div className="flex-1 flex flex-col relative bg-background/50">
                {activeChannel ? (
                    <ServerChatView />
                ) : (
                    <div className="flex-1 flex items-center justify-center">
                        <div className="text-center text-gray-500">
                            <Hash className="w-16 h-16 mx-auto mb-4 opacity-50" />
                            <p className="text-lg font-medium">Select a channel</p>
                            <p className="text-sm">Choose a channel to start chatting</p>
                        </div>
                    </div>
                )}
            </div>

            <MemberList />
        </>
    );
}
