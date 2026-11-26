<script>
  import { icon } from "#helpers/core.js";
  import { speaker } from "#state/panelState.js";

  let { current = "speaker" } = $props();

  let training = $derived(Boolean($speaker?.settings?.wake_training));

  let items = $derived([
    { id: "speaker", href: "/speaker", icon: "speaker", label: "Колонка" },
    { id: "connections", href: "/connections", icon: "puzzle", label: "Сервисы и ИИ" },
    { id: "charts", href: "/charts", icon: "activity", label: "Мониторинг" },
    { id: "logs", href: "/logs", icon: "terminal", label: "Журнал" },
    { id: "security", href: "/security", icon: "shield", label: "Бэкап и доступ" },
    ...(training ? [{ id: "training", href: "/training", icon: "mic", label: "Обучение" }] : []),
  ]);
</script>

<nav class="hub-tabs" aria-label="Разделы настроек">
  {#each items as item (item.id)}
    <a href={item.href} class="hub-tab" class:on={current === item.id}>
      {@html icon(item.icon)}
      <span>{item.label}</span>
    </a>
  {/each}
</nav>
