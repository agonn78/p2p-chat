import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import type { Components } from 'react-markdown';

interface MessageContentProps {
    content: string;
    isEncrypted?: boolean;
}

// Custom components for markdown rendering
const markdownComponents: Components = {
    // Style code blocks
    code: ({ node, className, children, ...props }) => {
        const isInline = !className;
        if (isInline) {
            return (
                <code className="bg-black/30 px-1.5 py-0.5 rounded text-pink-400 text-sm font-mono" {...props}>
                    {children}
                </code>
            );
        }
        return (
            <code className="block bg-black/40 p-3 rounded-lg my-2 text-sm font-mono overflow-x-auto" {...props}>
                {children}
            </code>
        );
    },
    // Style pre blocks
    pre: ({ children }) => (
        <pre className="bg-black/40 rounded-lg overflow-x-auto">{children}</pre>
    ),
    // Style links
    a: ({ children, href }) => (
        <a
            href={href}
            target="_blank"
            rel="noopener noreferrer"
            className="text-blue-400 hover:text-blue-300 underline"
        >
            {children}
        </a>
    ),
    // Style lists
    ul: ({ children }) => <ul className="list-disc list-inside ml-2 my-1">{children}</ul>,
    ol: ({ children }) => <ol className="list-decimal list-inside ml-2 my-1">{children}</ol>,
    // Style blockquotes
    blockquote: ({ children }) => (
        <blockquote className="border-l-4 border-gray-500 pl-3 my-2 italic text-gray-400">
            {children}
        </blockquote>
    ),
    // Style strong/bold
    strong: ({ children }) => <strong className="font-bold text-white">{children}</strong>,
    // Style emphasis/italic
    em: ({ children }) => <em className="italic">{children}</em>,
    // Style strikethrough
    del: ({ children }) => <del className="line-through text-gray-500">{children}</del>,
    // Style paragraphs (no extra margins)
    p: ({ children }) => <p className="my-0">{children}</p>,
};

export function MessageContent({ content, isEncrypted }: MessageContentProps) {
    // Don't render markdown for encrypted placeholders
    if (content === '[Encrypted]' || content === '[Decrypting...]') {
        return <span className="text-gray-500 italic">{content}</span>;
    }

    return (
        <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={markdownComponents}
            className="prose prose-invert prose-sm max-w-none break-words"
        >
            {content}
        </ReactMarkdown>
    );
}
