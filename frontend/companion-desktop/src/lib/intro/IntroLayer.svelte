<script lang="ts">
  /**
   * IntroLayer.svelte — 开场动画「火之文明史」覆盖层（Svelte 5 封装 IntroFire 引擎）
   *
   * 全屏 fixed 覆盖一切（z-index 在 chrome 之上），自带 WebGL2 canvas，驱动 fire.ts
   * 按 timeline.ts 十拍播放。拍 1–5 火心钉屏幕中心（"同一簇火"匹配剪辑），拍 5 起
   * origin 随子弹加速，拍 8 划月，拍 9 金线弧线划向行星/星环方向。
   *
   * 落幅接缝（核心卖点）：最后 1.5s（landing 拍）本层整体淡出——底下 SceneLayer /
   * PlanetLayer / BridgeLayer 全程挂载从不卸载，淡出中途活舰桥显形；播完 onComplete
   * 由 App 卸载本层（引擎 destroy → loseContext 释放 GL）。
   *
   * 交互：点击或 Esc = 快进到落幅淡出（引擎 skip()；?it= 冻结调试下无效）。
   * 门禁（reduced-motion / ap-intro-seen / ?intro=1 / ?it=）在 App.svelte 侧裁决，
   * 本组件被挂载即播放。
   */
  import {onMount} from 'svelte';
  import {IntroFire} from './fire';
  import {INTRO_LANDING_START, INTRO_TOTAL} from './timeline';

  let {onComplete}: {onComplete?: () => void} = $props();

  let rootEl: HTMLDivElement | undefined;
  let canvasEl: HTMLCanvasElement | undefined;
  let engine: IntroFire | null = null;

  onMount(() => {
    if (!canvasEl || !rootEl) return;
    const el = rootEl;
    engine = new IntroFire(canvasEl, {
      onProgress(t) {
        // 落幅：INTRO_LANDING_START 起最后 1.5s 整层 JS 驱动淡出（合成属性，不 reflow），
        // 与 landing 拍同步——淡出中途底下活舰桥（黑洞+行星星环+舰桥内装）显形。
        if (t >= INTRO_LANDING_START) {
          const f = Math.min(1, (t - INTRO_LANDING_START) / (INTRO_TOTAL - INTRO_LANDING_START));
          el.style.opacity = String(1 - f);
        }
      },
      onComplete() {
        onComplete?.();
      },
    });
    return () => {
      engine?.destroy();
      engine = null;
    };
  });

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') engine?.skip();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- 点击 = 快进到落幅淡出（跳过纪律，storyboard §可跳过） -->
<div bind:this={rootEl} class="intro-layer" onclick={() => engine?.skip()} role="presentation">
  <canvas bind:this={canvasEl} class="intro-canvas" aria-hidden="true"></canvas>
</div>

<style>
  /* z 序在一切之上（chrome z≤3）；普通合成层，不参与下层任何混合——
     .planet-xfade 纪律只管定位祖先，本层不是 PlanetLayer 祖先，不破坏 screen 混合 */
  .intro-layer {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: var(--ap-space-void, #07070c);
    cursor: pointer;
  }
  .intro-canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
