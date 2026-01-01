import { createSignal, createResource, Show } from "solid-js";
import { SessionList } from "./components/SessionList";
import { ChatView } from "./components/ChatView";

export interface Session {
  id: string;
  format: string;
  age: string;
}

export interface LogEntry {
  type: string;
  message?: { content: ContentBlock[] | string; role?: string };
  payload?: Payload;
  summary?: string;
}

export interface ContentBlock {
  type: string;
  text?: string;
  name?: string;
  input?: unknown;
  content?: string | { text: string }[];
  is_error?: boolean;
}

export interface Payload {
  type: string;
  role?: string;
  content?: { text?: string }[];
  name?: string;
  arguments?: string;
  output?: string;
}

async function fetchSessions(): Promise<Session[]> {
  const res = await fetch("/api/sessions");
  return res.json();
}

async function fetchSession(id: string): Promise<LogEntry[]> {
  if (!id) return [];
  const res = await fetch(`/api/session/${id}`);
  return res.json();
}

export function App() {
  const [selectedId, setSelectedId] = createSignal<string>("");
  const [sessions] = createResource(fetchSessions);
  const [entries] = createResource(selectedId, fetchSession);

  return (
    <div class="app">
      <aside class="app__sidebar">
        <h1 class="app__title">Sessions</h1>
        <Show when={sessions()} fallback={<p class="app__loading">Loading...</p>}>
          <SessionList sessions={sessions()!} selectedId={selectedId()} onSelect={setSelectedId} />
        </Show>
      </aside>
      <main class="app__main">
        <Show when={selectedId()} fallback={<p class="app__placeholder">Select a session</p>}>
          <Show when={entries()} fallback={<p class="app__loading">Loading...</p>}>
            <ChatView entries={entries()!} />
          </Show>
        </Show>
      </main>
    </div>
  );
}
