import { micromark } from "micromark";
import { gfm, gfmHtml } from "micromark-extension-gfm";

export function renderMarkdown(text: string): string {
  try {
    return micromark(text, {
      extensions: [gfm()],
      htmlExtensions: [gfmHtml()],
    });
  } catch {
    // Fallback: escape HTML and preserve newlines
    return text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\n/g, "<br>");
  }
}
