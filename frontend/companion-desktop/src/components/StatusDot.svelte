<script lang="ts">
  let {
    size = 'normal',
    off = false,
    active = false,
  }: {
    size?: 'tiny' | 'small' | 'normal' | 'large';
    off?: boolean;
    active?: boolean;
  } = $props();
</script>

<span
  class:off
  class:active
  class:tiny={size === 'tiny'}
  class:small={size === 'small'}
  class:large={size === 'large'}
  class="status-dot"
  aria-hidden="true"
></span>
