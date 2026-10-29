<script lang="ts">
  import Icon from "../components/Icon.svelte";

  let { icon, title, text }: { icon: string; title: string; text: string } = $props();
</script>

<section class="upcoming">
  <Icon name={icon} size={34} />
  <h2>{title}</h2>
  <p>{text}</p>
</section>

<style>
  .upcoming {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 32px;
    text-align: center;
    color: var(--text-faint);
  }
  h2 {
    margin: 10px 0 0;
    font-size: 18px;
    color: var(--text);
  }
  p {
    margin: 0;
    max-width: 420px;
    line-height: 1.5;
  }
</style>
