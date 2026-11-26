<script module>
  const messageClasses = {
    day: "svc day",
    service: "svc",
    message: "b me",
    listening: "b me live",
    answer: "b him ans",
    thinking: "b him think",
    speaking: "b him speak",
    error: "b him err",
  };
  const messageLabels = {
    listening: "Говорите",
    thinking: "Джарвис думает",
    speaking: "Джарвис отвечает",
  };
</script>

<script>
  import { untrack } from "svelte";
  import { api, copyText, icon, toast } from "#helpers/core.js";
  import { animateAnswer } from "./animateAnswer.js";

  let { row, animate, follow } = $props();
  const entering = untrack(() => animate && row.kind !== "answer");
  let node = $state();
  let previousAnswer;
  let previousSize;
  let busyAct = $state("");

  async function copyAnswer() {
    try {
      await copyText(row.answer);
      toast("Ответ скопирован");
    } catch {
      toast("Не удалось скопировать", true);
    }
  }

  async function speakAnswer() {
    if (busyAct) return;
    busyAct = "speak";
    try {
      await api("POST", "/api/say", { text: row.answer, direct: true });
      toast("Озвучиваю на колонке");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busyAct = "";
    }
  }

  async function saveToNotes() {
    if (busyAct) return;
    busyAct = "note";
    try {
      await api("POST", "/api/notes/save", {
        title: "",
        content: row.answer,
        tags: ["чат"],
        folder: "Голосовые заметки",
      });
      toast("Сохранено в CyberNotes");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busyAct = "";
    }
  }

  $effect.pre(() => {
    if (row.answer === previousAnswer) return;
    previousAnswer = row.answer;
    previousSize = node?.isConnected ? { w: node.offsetWidth, h: node.offsetHeight } : undefined;
  });
</script>

<div bind:this={node} class={messageClasses[row.kind]} class:new={entering && row.kind !== "answer"} aria-label={messageLabels[row.kind]}>
  {#if row.kind === "day" || row.kind === "service"}
    {row.text}
  {:else if row.kind === "message" || row.kind === "error"}
    <span class="t">{row.text}</span>
    <span class="m">
      {row.time}
      {#if row.kind === "message"}{@html icon(row.answered ? "read" : "sent")}{/if}
    </span>
  {:else if row.kind === "listening"}
    <span class="wave" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i><i></i><i></i></span>
    <span class="t">Слушаю</span>
  {:else if row.kind === "thinking"}
    <span class="think-t">Думаю</span>
  {:else if row.kind === "speaking"}
    <span class="eqw" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>
    <span class="t">Отвечает</span>
  {:else if row.kind === "answer"}
    <span class="t" use:animateAnswer={{ text: row.answer, animate, follow, measure: () => previousSize }}></span>
    <span class="m">
      {row.time}
      <button type="button" class="b-copy" disabled={busyAct === "speak"} onclick={speakAnswer} title="Озвучить на колонке" aria-label="Озвучить на колонке">{@html icon("voice")}</button>
      <button type="button" class="b-copy" disabled={busyAct === "note"} onclick={saveToNotes} title="Сохранить в CyberNotes" aria-label="Сохранить в CyberNotes">{@html icon("note")}</button>
      <button type="button" class="b-copy" onclick={copyAnswer} title="Скопировать ответ" aria-label="Скопировать ответ">{@html icon("copy")}</button>
    </span>
  {/if}
</div>
