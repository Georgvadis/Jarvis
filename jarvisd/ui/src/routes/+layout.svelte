<script>
  import App from "#components/App.svelte";
  import "../app.css";
  import "../warm-homeos.css";

  let { children } = $props();
</script>

<App {children} />
