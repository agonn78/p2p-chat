import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface MessageContentProps {
    content: string;
    isEncrypted?: boolean;
}

export function MessageContent({ content, isEncrypted }: MessageContentProps) {
    // Don't render markdown for encrypted placeholders
    if (content === '[Encrypted]' || content === '[Decrypting...]') {
        return <span className="text-gray-500 italic">{content}</span>;
    }

    return (
        <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{
                // Style code - inline and block
                code(props) {
                    const { children, className, node, ...rest } = props;
                    const isBlock = className?.includes('language-');
                    if (isBlock) {
                        return (
                            <code
                                {...rest}
                                className="block bg-black/40 p-3 rounded-lg my-2 text-sm font-mono overflow-x-auto text-gray-200"
                            >
                                {children}
                            </code>
                        );
                    }
                    return (
                        <code
                            {...rest}
                            className="bg-black/30 px-1.5 py-0.5 rounded text-pink-400 text-sm font-mono"
                        >
                            {children}
                        </code>
                    );
                },
                // Style pre blocks
                pre(props) {
                    const { children, ...rest } = props;
                    return (
                        <pre {...rest} className="bg-black/40 rounded-lg overflow-x-auto">
                            {children}
                        </pre>
                    );
                },
                // Style links
                a(props) {
                    const { children, href, ...rest } = props;
                    return (
                        <a
                            {...rest}
                            href={href}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="text-blue-400 hover:text-blue-300 underline"
                        >
                            {children}
                        </a>
                    );
                },
                // Style lists
                ul(props) {
                    const { children, ...rest } = props;
                    return <ul {...rest} className="list-disc list-inside ml-2 my-1">{children}</ul>;
                },
                ol(props) {
                    const { children, ...rest } = props;
                    return <ol {...rest} className="list-decimal list-inside ml-2 my-1">{children}</ol>;
                },
                // Style blockquotes
                blockquote(props) {
                    const { children, ...rest } = props;
                    return (
                        <blockquote {...rest} className="border-l-4 border-gray-500 pl-3 my-2 italic text-gray-400">
                            {children}
                        </blockquote>
                    );
                },
                // Style strong/bold
                strong(props) {
                    const { children, ...rest } = props;
                    return <strong {...rest} className="font-bold text-white">{children}</strong>;
                },
                // Style emphasis/italic
                em(props) {
                    const { children, ...rest } = props;
                    return <em {...rest} className="italic">{children}</em>;
                },
                // Style strikethrough
                del(props) {
                    const { children, ...rest } = props;
                    return <del {...rest} className="line-through text-gray-500">{children}</del>;
                },
                // Style paragraphs (no extra margins)
                p(props) {
                    const { children, ...rest } = props;
                    return <p {...rest} className="my-0">{children}</p>;
                },
            }}
        >
            {content}
        </ReactMarkdown>
    );
}
