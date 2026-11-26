import { Motion } from "#helpers/core.js";
import { renderMarkdownInto } from "#helpers/markdown.js";

// Спокойное и чёткое отображение ответа ассистента:
// Markdown форматируется сразу без прыгающей высоты, размытия и «взлетающих» слов.
export function animateAnswer(node, options) {
  let prevText = "";
  let firstPaint = true;

  function update({ text, animate, follow }) {
    const next = String(text || "");
    if (next === prevText) return;
    prevText = next;

    renderMarkdownInto(node, next);

    if (firstPaint && animate && !Motion.reduce) {
      const bubble = node.parentElement;
      if (bubble) {
        bubble.classList.remove("ans-fade");
        void bubble.offsetWidth;
        bubble.classList.add("ans-fade");
      }
    }
    firstPaint = false;
    follow?.();
  }

  update(options);
  return {
    update,
    destroy() {
      node.parentElement?.classList.remove("ans-fade");
    },
  };
}
