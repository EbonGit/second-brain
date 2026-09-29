import { useState, useRef, useEffect } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { useBrainStore } from '../store';
import './ChatPanel.css';

function ChatMessage({ content }: { content: string }) {
  const { createNote } = useBrainStore();

  const components = {
    h2: ({ children, ...props }: any) => {
      const text = typeof children === 'string' ? children
        : Array.isArray(children) ? children.filter(c => typeof c === 'string').join('') : '';
      const match = text.match(/^Nouvelle note[:\s]+(.+)/i);
      if (match) {
        const noteTitle = match[1].trim();
        return (
          <h2 {...props} className="jb-note-proposal-heading">
            {children}
            <button
              className="jb-create-note-btn"
              onClick={() => createNote('note', noteTitle)}
              title={`Créer la note «${noteTitle}»`}
            >
              + Créer
            </button>
          </h2>
        );
      }
      return <h2 {...props}>{children}</h2>;
    },
  };

  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
      {content}
    </ReactMarkdown>
  );
}

export default function ChatPanel() {
  const { chatHistory, chatLoading, sendChat, clearChat, settings, setShowSettings, claudeUsage } = useBrainStore();
  const [input, setInput] = useState('');
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [chatHistory, chatLoading]);

  const useCli = settings.use_claude_cli ?? true;
  const hasKey = !!settings.anthropic_api_key;
  const ready = useCli || hasKey;

  const handleSend = async () => {
    const msg = input.trim();
    if (!msg || chatLoading) return;
    setInput('');
    await sendChat(msg);
  };

  const handleKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend(); }
  };

  if (!ready) {
    return (
      <div className="jb-chat-empty">
        <span className="jb-chat-empty-icon">◈</span>
        <p>Configure Claude pour activer le chat</p>
        <button className="jb-chat-config-btn" onClick={() => setShowSettings(true)}>
          Ouvrir Réglages
        </button>
      </div>
    );
  }

  return (
    <div className="jb-chat">
      <div className="jb-chat-toolbar">
        <span className="jb-chat-title">Chat · Vault</span>
        {claudeUsage && !chatLoading && (
          <span className="jb-token-usage" title="Tokens de la dernière requête Claude">
            ↑{claudeUsage.input.toLocaleString()} ↓{claudeUsage.output} tok
          </span>
        )}
        {chatHistory.length > 0 && (
          <button className="jb-chat-clear" onClick={clearChat} title="Effacer la conversation">
            ✕ Effacer
          </button>
        )}
      </div>

      <div className="jb-chat-messages">
        {chatHistory.length === 0 && (
          <div className="jb-chat-hint">
            Pose une question sur tes notes, demande des connexions entre idées, ou explore un concept.
          </div>
        )}
        {chatHistory.map((msg, i) => (
          <div key={i} className={`jb-chat-msg jb-chat-msg--${msg.role}`}>
            <div className="jb-chat-msg-role">
              {msg.role === 'user' ? 'Toi' : 'Claude'}
            </div>
            <div className="jb-chat-msg-body">
              {msg.role === 'assistant'
                ? <ChatMessage content={msg.content} />
                : <ReactMarkdown remarkPlugins={[remarkGfm]}>{msg.content}</ReactMarkdown>
              }
            </div>
          </div>
        ))}
        {chatLoading && (
          <div className="jb-chat-msg jb-chat-msg--assistant">
            <div className="jb-chat-msg-role">Claude</div>
            <div className="jb-chat-thinking">
              <span /><span /><span />
            </div>
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      <div className="jb-chat-input-row">
        <textarea
          className="jb-chat-input"
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={handleKey}
          placeholder="Envoyer un message... (Entrée pour envoyer, Maj+Entrée pour saut de ligne)"
          rows={2}
          disabled={chatLoading}
        />
        <button
          className="jb-chat-send"
          onClick={handleSend}
          disabled={!input.trim() || chatLoading}
        >
          ↑
        </button>
      </div>
    </div>
  );
}
