<script lang="ts">
  import { store } from './store.svelte';
</script>

<div class="toasts">
  {#each store.toasts as t (t.id)}
    <div class="toast {t.kind}">{t.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: 1rem;
    right: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    z-index: 200;
    max-width: 380px;
  }
  .toast {
    padding: 0.6rem 0.9rem;
    border-radius: 8px;
    font-size: 0.8rem;
    border: 1px solid var(--border);
    background: var(--bg-menu);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
    animation: slide-in 0.15s ease-out;
  }
  .toast.ok {
    border-left: 3px solid var(--ok);
  }
  .toast.error {
    border-left: 3px solid var(--danger);
  }
  @keyframes slide-in {
    from { transform: translateX(20px); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }
</style>
