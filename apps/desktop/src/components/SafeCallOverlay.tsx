import { Component, ReactNode } from 'react';
import { CallOverlay } from './CallOverlay';

interface Props {
    resetKey: string;
}

interface State {
    hasError: boolean;
}

export class SafeCallOverlay extends Component<Props, State> {
    state: State = { hasError: false };

    static getDerivedStateFromError(): State {
        return { hasError: true };
    }

    componentDidCatch(error: unknown) {
        console.error('[SafeCallOverlay] Render error:', error);
    }

    componentDidUpdate(prevProps: Props) {
        if (this.state.hasError && prevProps.resetKey !== this.props.resetKey) {
            this.setState({ hasError: false });
        }
    }

    render(): ReactNode {
        if (this.state.hasError) {
            return null;
        }

        return <CallOverlay />;
    }
}
