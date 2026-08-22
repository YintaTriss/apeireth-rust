<script lang="ts">
  /**
   * SceneLayer.svelte — 场景层组件（Svelte 5 封装 BlackholeScene 引擎）
   *
   * 全屏绝对定位铺底；z-index 不设，由调用方控制层序（规范 §5.1：场景层在 z 最低）。
   *
   * Props:
   *   presence        —— { p, a, d, mode } | null；null 时用默认（quiet + PAD 0）。
   *                        mode 映射到原型状态机；p/a/d 为 v1 简化映射（亮度 += p*0.15、
   *                        转速 += a*0.4，数值 TODO 待实拍校准，规范 §4.1/B-2）。
   *                        形状与将来的 src/lib/presence.ts presenceStore 订阅值对齐。
   *   hour            —— 舰内时刻 0-24；null 时跟随本地时钟。
   *   interactive     —— 是否响应鼠标轨道视差（页面层打开时调用方关掉）。
   *   onBlackholeClick —— 点击黑洞区域（光子环 1.05R 以内）时回调；组件内部同时自管理
   *                        切到「临渊」机位（规范 §4.2 已验收行为）。
   *
   * 内部交互自管理：机位快捷键 1/2/3（输入框聚焦时不触发——原型无文本输入，
   * 应用内需避让）；reduced-motion 锁定远眺机位（引擎内处理）。
   */
  import {onMount} from 'svelte';
  import {BlackholeScene} from './blackhole';
  import type {FallbackVisuals, PresenceInput} from './blackhole';

  let {
    presence = null,
    hour = null,
    interactive = true,
    cameraIndex = null,
    onBlackholeClick,
  }: {
    presence?: PresenceInput | null;
    hour?: number | null;
    interactive?: boolean;
    /** 受控机位（波次 4 三模式）：null = 引擎内部自管理（快捷键/点黑洞，现状不变）；
     *  非 null 时按值调 engine.setCamera。reduced-motion 下引擎仍锁定远眺，纪律不变。 */
    cameraIndex?: number | null;
    onBlackholeClick?: () => void;
  } = $props();

  let rootEl: HTMLDivElement | undefined;
  let canvasEl: HTMLCanvasElement | undefined;
  let engine: BlackholeScene | null = null;

  // Canvas 2D 降级路径专用 DOM 视觉（WebGL 路径由 shader uniform 承担，保持 0 不可见）
  let fbPulse = $state<{x: number; y: number; diameter: number} | null>(null);
  let fbAmbOpacity = $state(0);
  let fbEclipseOpacity = $state(0);

  function applyPresence(p: PresenceInput | null): void {
    if (!engine) return;
    engine.setMode(p?.mode ?? 'quiet');
    engine.setPadBias({p: p?.p ?? 0, a: p?.a ?? 0, d: p?.d ?? 0});
  }

  onMount(() => {
    if (!canvasEl) return;
    engine = new BlackholeScene(canvasEl, {
      interactive,
      onFallbackVisuals(v: FallbackVisuals) {
        fbPulse = v.pulse;
        fbAmbOpacity = v.ambOpacity;
        fbEclipseOpacity = v.eclipseOpacity;
      },
    });
    engine.setHour(hour);
    if (cameraIndex !== null) engine.setCamera(cameraIndex); // 受控机位首屏即生效（如 ?mode=focus 截图）
    applyPresence(presence);
    return () => {
      engine?.destroy();
      engine = null;
    };
  });

  $effect(() => {
    engine?.setHour(hour);
  });
  $effect(() => {
    engine?.setInteractive(interactive);
  });
  $effect(() => {
    applyPresence(presence);
  });
  $effect(() => {
    if (cameraIndex !== null) engine?.setCamera(cameraIndex);
  });

  function handleSceneClick(e: MouseEvent): void {
    if (!engine || !rootEl) return;
    const rect = rootEl.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const c = engine.holeScreenCircle();
    const rr = c.r * 1.05; // 光子环外沿（ring @1.045R）以内视为「黑洞区域」
    const dx = x - c.x;
    const dy = y - c.y;
    if (dx * dx + dy * dy <= rr * rr) {
      engine.setCamera(1); // §4.2：点击黑洞 = 切到「临渊」机位（reduced-motion 时引擎内锁定远眺）
      onBlackholeClick?.();
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (!engine || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
    const t = e.target as HTMLElement | null;
    // 应用内有聊天/设置等文本输入——输入时不响应机位快捷键（原型无此场景，属应用化适配）
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
    if (e.key === '1') engine.setCamera(0);
    else if (e.key === '2') engine.setCamera(1);
    else if (e.key === '3') engine.setCamera(2);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div bind:this={rootEl} class="scene-layer" onclick={handleSceneClick} role="presentation">
  <canvas bind:this={canvasEl} class="scene-canvas" aria-hidden="true"></canvas>
  <!-- 降级路径叠加层（opacity 由引擎回调驱动；WebGL 路径恒 0） -->
  <div class="amb-overlay" style:opacity={fbAmbOpacity} aria-hidden="true"></div>
  <div class="eclipse-overlay" style:opacity={fbEclipseOpacity} aria-hidden="true"></div>
  {#if fbPulse}
    <div
      class="pulse"
      style:left={`${fbPulse.x}px`}
      style:top={`${fbPulse.y}px`}
      style:width={`${fbPulse.diameter}px`}
      style:height={`${fbPulse.diameter}px`}
      aria-hidden="true"
    ></div>
  {/if}
</div>

<style>
  /* 全屏绝对定位铺底；不设 z-index（层序由调用方控制） */
  .scene-layer {
    position: absolute;
    inset: 0;
    overflow: hidden;
    /* tokens.css（--ap-* 命名空间）就位后承接 --ap-space-void；未就位时回退令牌值 color.void.base */
    background: var(--ap-space-void, #07070c);
  }
  .scene-canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }

  /* ---- Canvas 2D 降级路径专用 DOM 视觉（数值与原型 #amb-overlay/#eclipse-overlay/#pulse 一致） ---- */
  .amb-overlay,
  .eclipse-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    opacity: 0;
  }
  .amb-overlay {
    mix-blend-mode: screen;
    background: linear-gradient(to bottom, rgba(107, 140, 235, 0.55), rgba(70, 95, 170, 0.28) 55%, rgba(70, 95, 170, 0.12));
  }
  .eclipse-overlay {
    background: #020205;
  }
  .pulse {
    position: absolute;
    pointer-events: none;
    border-radius: 50%;
    filter: blur(5px);
    background: radial-gradient(closest-side, rgba(255, 243, 214, 0.5), rgba(255, 243, 214, 0) 70%);
    /* 原型仅在 keyframes 内 translate(-50%,-50%)，reduced-motion 下会失去居中——此处补齐基础 transform，
       动画期间 keyframes 覆盖（值相同），行为与原型一致 */
    transform: translate(-50%, -50%);
    animation: breathe 2.8s ease-in-out infinite;
  }
  @keyframes breathe {
    0%,
    100% {
      opacity: 0.22;
      transform: translate(-50%, -50%) scale(1);
    }
    50% {
      opacity: 0.65;
      transform: translate(-50%, -50%) scale(1.09);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse {
      animation: none;
      opacity: 0.45;
    }
  }
</style>
