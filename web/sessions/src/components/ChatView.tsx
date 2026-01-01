import { For, Show } from "solid-js";
import type { LogEntry, ContentBlock, Payload } from "../App";
import { renderMarkdown } from "../markdown";

interface Props {
  entries: LogEntry[];
}

export function ChatView(props: Props) {
  return (
    <div class="chat-view">
      <For each={props.entries}>{(entry) => <Entry entry={entry} />}</For>
    </div>
  );
}

function Entry(props: { entry: LogEntry }) {
  const entry = props.entry;
  const type = entry.type;

  // Claude Code format
  if (type === "user" || type === "assistant") {
    const content = entry.message?.content;
    if (!content) return null;

    // Handle content as string
    if (typeof content === "string") {
      return (
        <div class={`message message--${type}`}>
          <div class="message__role">{type}</div>
          <div class="message__content">
            <div class="message__text" innerHTML={renderMarkdown(content)} />
          </div>
        </div>
      );
    }

    // Handle content as array of blocks
    return (
      <div class={`message message--${type}`}>
        <div class="message__role">{type}</div>
        <div class="message__content">
          <For each={content}>{(block) => <ContentBlockView block={block} />}</For>
        </div>
      </div>
    );
  }

  if (type === "summary") {
    return (
      <div class="message message--summary">
        <div class="message__role">Summary</div>
        <pre class="message__text">{entry.summary}</pre>
      </div>
    );
  }

  // Codex format
  if (type === "response_item" || type === "event_msg") {
    const payload = entry.payload;
    if (!payload) return null;
    return <PayloadView payload={payload} />;
  }

  return null;
}

function ContentBlockView(props: { block: ContentBlock }) {
  const block = props.block;

  if (block.type === "text") {
    return <div class="message__text" innerHTML={renderMarkdown(block.text || "")} />;
  }

  if (block.type === "tool_use") {
    return <ToolUse name={block.name || "unknown"} input={block.input} />;
  }

  if (block.type === "tool_result") {
    let text = "";
    if (typeof block.content === "string") {
      text = block.content;
    } else if (Array.isArray(block.content)) {
      text = block.content.map((c) => c.text || "").join("\n");
    }
    return <ToolResult text={text} isError={block.is_error} />;
  }

  return null;
}

function PayloadView(props: { payload: Payload }) {
  const p = props.payload;

  if (p.type === "message") {
    const role = p.role || "unknown";
    return (
      <div class={`message message--${role === "user" ? "user" : "assistant"}`}>
        <div class="message__role">{role}</div>
        <div class="message__content">
          <For each={p.content || []}>
            {(block) => (
              <Show when={block.text}>
                <div class="message__text" innerHTML={renderMarkdown(block.text!)} />
              </Show>
            )}
          </For>
        </div>
      </div>
    );
  }

  if (p.type === "function_call") {
    return <ToolUse name={p.name || ""} input={p.arguments} />;
  }

  if (p.type === "function_call_output") {
    return <ToolResult text={p.output || ""} />;
  }

  return null;
}

function ToolUse(props: { name: string; input: unknown }) {
  // Parse input - could be string (JSON) or object
  let params: Record<string, unknown> = {};
  if (typeof props.input === "string") {
    try {
      params = JSON.parse(props.input);
    } catch {
      params = { raw: props.input };
    }
  } else if (props.input && typeof props.input === "object") {
    params = props.input as Record<string, unknown>;
  }

  const entries = Object.entries(params);

  return (
    <div class="tool-use">
      <div class="tool-use__header">
        <span class="tool-use__name">{props.name}</span>
      </div>
      <Show when={entries.length > 0}>
        <table class="tool-use__params">
          <tbody>
            <For each={entries}>
              {([key, value]) => (
                <tr class="tool-use__param">
                  <td class="tool-use__param-key">{key}</td>
                  <td class="tool-use__param-value">
                    <ParamValue value={value} />
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </div>
  );
}

function ParamValue(props: { value: unknown }) {
  const v = props.value;

  if (v === null || v === undefined) {
    return <span class="param-value--null">null</span>;
  }

  if (typeof v === "boolean") {
    return <span class="param-value--bool">{v ? "true" : "false"}</span>;
  }

  if (typeof v === "number") {
    return <span class="param-value--number">{v}</span>;
  }

  if (typeof v === "string") {
    // Multi-line strings get a code block
    if (v.includes("\n")) {
      return <pre class="param-value--code">{v}</pre>;
    }
    return <span class="param-value--string">{v}</span>;
  }

  // Objects/arrays: show as JSON
  return <pre class="param-value--json">{JSON.stringify(v, null, 2)}</pre>;
}

function ToolResult(props: { text: string; isError?: boolean }) {
  return (
    <div class="tool-result" classList={{ "tool-result--error": props.isError }}>
      <pre class="tool-result__output">{props.text}</pre>
    </div>
  );
}
