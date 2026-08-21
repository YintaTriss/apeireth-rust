<script lang="ts">
  import type {Snippet} from 'svelte';
  let {
    eyebrow,
    title,
    subtitle,
    children,
  }: {
    eyebrow: string;
    title: string;
    subtitle: string;
    children?: Snippet;
  } = $props();
</script>

<div class="page-head">
  <div>
    <p class="eyebrow">{eyebrow}</p>
    <h1>{title}</h1>
    <p class="subtitle">{subtitle}</p>
  </div>
  {#if children}{@render children()}{/if}
</div>
