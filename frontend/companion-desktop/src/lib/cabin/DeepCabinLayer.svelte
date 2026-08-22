<script lang="ts">
  /**
   * DeepCabinLayer.svelte — 深舱内装层（波次 4 三模式骨架：工程模式的家）
   *
   * 镜像 BridgeLayer 范式，差异仅在：
   *   - 深舱图整幅不透明 cover（无舷窗镂空）→ 照明叠加层无需 alpha mask，
   *     直接整幅叠加（isolation 隔离组保证 blend 背景仅为舱内图，不渗入下层场景）。
   *   - 工程模式下不透明盖住黑洞场景与行星层（两层保持挂载不断状态）；
   *     与 BridgeLayer 的交叉淡由调用方（App）按模式切 opacity 承担，本层只管渲染自己。
   *
   * 层序：与 BridgeLayer 同档（z=1），DOM 序在其后（深舱浮于舰桥之上，交叉淡时互不闪穿）。
   *
   * Props:
   *   hour —— 舰内时刻 0-24（支持小数），驱动时间线照明叠加（与舰桥同一时刻源）。
   *
   * 视差：与 BridgeLayer 同一头部转动物理模型（perspective 1400px、
   *   rotateY(-mx*1.0°)/rotateX(my*0.7°) + 残余平移，lerp 0.06，
   *   ?px=&py= 开发覆写，prefers-reduced-motion 全停）。
   */
  import {onMount} from 'svelte';
  import deepCabinWebp from '../../assets/cabin/deep-cabin.webp';
  import {normalizeHour, sstep} from '../scene/timeline';
  import {readParallaxOverride} from '../scene/parallax';

  let {
    hour = 12,
  }: {
    hour?: number;
  } = $props();

  let frameEl = $state<HTMLDivElement>();

  // ---------- 时间线照明（曲线与 BridgeLayer 完全一致：同一舰内时刻、同一舱内光纪律） ----------
  const lighting = $derived.by(() => {
    const h = normalizeHour(hour);
    const dawn = sstep(4.5, 8.0, h);
    const dusk = sstep(14.0, 18.0, h);
    const night = 1 - dawn * (1 - dusk);
    const cross = h >= 12 ? sstep(16.5, 18.5, h) : 1;
    const r = Math.round(120 + (10 - 120) * cross);
    const g = Math.round(72 + (18 - 72) * cross);
    const b = Math.round(20 + (40 - 20) * cross);
    return {
      multiplyColor: `rgb(${r}, ${g}, ${b})`,
      multiplyOpacity: night * (0.28 + (0.45 - 0.28) * cross),
      brightness: 1 - 0.38 * night,
      screenOpacity: 1 - sstep(0.2, 0.3334, Math.abs(h - 12)),
    };
  });

  onMount(() => {
    const frame = frameEl;
    if (!frame) return;

    // reduced-motion：视差全停（规范 §7.1）
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (reduceMotion) return;

    // 开发覆写 ?px=&py=：两轴齐备且有限时锁定视差目标，不跟随鼠标
    const override = readParallaxOverride();

    let mx = 0;
    let my = 0;
    let mxTarget = 0;
    let myTarget = 0;
    let raf: number | null = null;

    function tick(): void {
      raf = null;
      mx += (mxTarget - mx) * 0.06;
      my += (myTarget - my) * 0.06;
      frame!.style.transform =
        `perspective(1400px) rotateX(${(my * 0.7).toFixed(3)}deg) ` +
        `rotateY(${(-mx * 1.0).toFixed(3)}deg) ` +
        `translate3d(${(-mx * 4).toFixed(2)}px, ${(-my * 2.5).toFixed(2)}px, 0)`;
      raf = requestAnimationFrame(tick);
    }

    function onMove(e: MouseEvent): void {
      mxTarget = Math.min(1, Math.max(-1, (e.clientX / window.innerWidth) * 2 - 1));
      myTarget = Math.min(1, Math.max(-1, (e.clientY / window.innerHeight) * 2 - 1));
    }

    function onLeave(): void {
      mxTarget = 0;
      myTarget = 0;
    }

    if (override) {
      mxTarget = override.x;
      myTarget = override.y;
    } else {
      window.addEventListener('mousemove', onMove, {passive: true});
      document.documentElement.addEventListener('mouseleave', onLeave);
      window.addEventListener('blur', onLeave);
    }
    raf = requestAnimationFrame(tick);

    return () => {
      if (!override) {
        window.removeEventListener('mousemove', onMove);
        document.documentElement.removeEventListener('mouseleave', onLeave);
        window.removeEventListener('blur', onLeave);
      }
      if (raf !== null) cancelAnimationFrame(raf);
    };
  });
</script>

<div class="cabin-layer" aria-hidden="true">
  <div class="cabin-frame" bind:this={frameEl}>
    <div class="cabin-interior" style:filter={`brightness(${lighting.brightness.toFixed(3)})`}>
      <img class="cabin-img" src={deepCabinWebp} alt="" draggable="false" />
      <div
        class="overlay multiply"
        style:opacity={lighting.multiplyOpacity.toFixed(3)}
        style:background-color={lighting.multiplyColor}
      ></div>
      <div class="overlay screen-glow" style:opacity={lighting.screenOpacity.toFixed(3)}></div>
    </div>
  </div>
</div>

<style>
  /* 与 BridgeLayer 同档（z=1），DOM 序在其后；不截获任何指针事件。
     交叉淡的 opacity/pointer-events 由调用方按模式控制（本层常驻 DOM）。 */
  .cabin-layer {
    position: fixed;
    inset: 0;
    z-index: 1;
    overflow: hidden;
    pointer-events: none;
  }
  /* 视差容器：出血 -4.5%，与舰桥同模型同出血量 */
  .cabin-frame {
    position: absolute;
    inset: -4.5%;
    will-change: transform;
    transform-origin: 50% 50%;
  }
  /* 内装隔离组：深舱图整幅不透明，blend 背景即舱内图本身；
     isolation 保证不渗入下层黑洞/行星场景。 */
  .cabin-interior {
    position: absolute;
    inset: 0;
    isolation: isolate;
    transition: filter var(--ap-dur-timeline, 90s) linear;
  }
  .cabin-img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    user-select: none;
  }
  /* 无镂空 → 无需 alpha mask，整幅叠加 */
  .overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    transition:
      opacity var(--ap-dur-timeline, 90s) linear,
      background-color var(--ap-dur-timeline, 90s) linear;
  }
  .multiply {
    mix-blend-mode: multiply;
  }
  /* 凌日暖金：方位与舰桥一致（上方中央泻入），保持两舱同一光源语言 */
  .screen-glow {
    mix-blend-mode: screen;
    background: radial-gradient(
      ellipse 62% 55% at 50% 30%,
      rgba(255, 210, 122, 0.18),
      rgba(255, 210, 122, 0.05) 55%,
      transparent 78%
    );
  }
</style>
