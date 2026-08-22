<script lang="ts">
  /**
   * BridgeLayer.svelte — 舰桥内装层（波次 3 建；3e 拆除全息环、视差升级为头部转动模型）
   *
   * 层序：SceneLayer（z 最低，黑洞星空）< PlanetLayer（中景环平原）< BridgeLayer（z=1，
   * 舰桥内装，全景舷窗镂空处露出场景）< bridge-ui（z=2，导航 + 对话）。
   *
   * 结构（.bridge-frame 视差容器内部）：
   *   .bridge-interior（隔离组，承载照明滤镜）
   *     img            —— bridge-back.webp 内装图，object-fit:cover（2048×1088 基准）
   *     .multiply      —— 时间线照明 multiply 叠加（琥珀暮色 → 深蓝夜航）
   *     .screen-glow   —— 凌日暖金径向光（screen）
   *
   * 两个照明叠加层用同一张 webp 做 alpha mask（mask-size:cover 与 img 几何一致），
   * 只给内装打光、不碰舷窗里的黑洞场景（场景层有自己的时间线照明）。
   *
   * Props:
   *   hour —— 舰内时刻 0-24（支持小数），驱动时间线照明叠加。
   *
   * 视差（波次 3e 头部转动物理模型，替代纯二维平移）：
   *   perspective(1400px) + rotateY(-mx*1.0°) 偏航 + rotateX(my*0.7°) 俯仰
   *   + 残余平移 translate3d(-mx*4px, -my*2.5px, 0)。
   *   符号约定（物理推导：内容按头部转动的逆旋转）：
   *     鼠标右移 = 向右看 → 舱室右缘向观察者逼近、内容微左移 → rotateY(-mx)，translate(-mx)；
   *     鼠标下移 = 向下看 → 舱室底缘逼近、内容微上移 → rotateX(+my)，translate(-my)。
   *   lerp 0.06/frame 平滑，鼠标离窗/失焦回中；旋转中心 = 容器中心；
   *   出血 inset:-4.5% 覆盖旋转边缘收缩（1°@1400px 半宽收缩 ~9px + 平移 4px ≪ 72px）。
   *   开发调试：?px=1&py=-1 强制视差到指定归一化位置（../scene/parallax.ts）。
   *   prefers-reduced-motion → 视差全停，静态一帧。
   */
  import {onMount} from 'svelte';
  import bridgeBackWebp from '../../assets/bridge/bridge-back.webp';
  import {normalizeHour, sstep} from '../scene/timeline';
  import {readParallaxOverride} from '../scene/parallax';

  let {
    hour = 12,
  }: {
    hour?: number;
  } = $props();

  let frameEl = $state<HTMLDivElement>();

  // ---------- 时间线照明（hour → 两组叠加参数；边界全平滑，无硬跳变） ----------
  // night：0=行照，14:00–18:00 坡道升入夜航平台，04:30–08:00 黎明坡道回落。
  // cross：multiply 色彩琥珀(120,72,20) → 深蓝(10,18,40) 的入夜过渡（17:30–19:30）。
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
      brightness: 1 - 0.38 * night, // 夜航熄灯影院 → 0.62
      // 凌日 12:00 ±20min 窗口（曲线与场景层 timeline.ts 一致）
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
      // lerp 0.06/frame 平滑趋近目标（手感与旧平移模型一致）
      mx += (mxTarget - mx) * 0.06;
      my += (myTarget - my) * 0.06;
      // 头部转动模型：偏航/俯仰微旋转为主，残余小平移为辅
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

<div class="bridge-layer" aria-hidden="true">
  <div class="bridge-frame" bind:this={frameEl}>
    <div class="bridge-interior" style:filter={`brightness(${lighting.brightness.toFixed(3)})`}>
      <img class="bridge-img" src={bridgeBackWebp} alt="" draggable="false" />
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
  /* 中间层：SceneLayer/PlanetLayer 之上、bridge-ui（z=2）之下；不截获任何指针事件 */
  .bridge-layer {
    position: fixed;
    inset: 0;
    z-index: 1;
    overflow: hidden;
    pointer-events: none;
  }
  /* 视差容器：出血 -4.5%（波次 3e 从 -3% 加大）——头部转动模型的旋转边缘收缩
     （1° 偏航 @1400px 半宽收缩 ~9px，俯仰 ~2px）+ 残余平移 4px 远在出血量内 */
  .bridge-frame {
    position: absolute;
    inset: -4.5%;
    will-change: transform;
    transform-origin: 50% 50%;
  }
  /* 内装隔离组：夜航 brightness 滤镜只作用于内装 + 照明叠加；
     isolation 保证 blend 的背景仅为本组内容（内装图），不渗入场景层。 */
  .bridge-interior {
    position: absolute;
    inset: 0;
    isolation: isolate;
    /* 照明过渡慢性子（规范 §3.3，令牌 motion.timeline ≥60s，取默认 90s） */
    transition: filter var(--ap-dur-timeline, 90s) linear;
  }
  .bridge-img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    user-select: none;
  }
  /* 照明叠加：用同一张 webp 的 alpha 做 mask（cover/居中，与 img 几何一致），
     只给舰桥内装打光，舷窗镂空处的黑洞场景不受叠加影响 */
  .overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    -webkit-mask-image: url('../../assets/bridge/bridge-back.webp');
    mask-image: url('../../assets/bridge/bridge-back.webp');
    -webkit-mask-size: cover;
    mask-size: cover;
    -webkit-mask-position: center;
    mask-position: center;
    -webkit-mask-repeat: no-repeat;
    mask-repeat: no-repeat;
    transition:
      opacity var(--ap-dur-timeline, 90s) linear,
      background-color var(--ap-dur-timeline, 90s) linear;
  }
  .multiply {
    mix-blend-mode: multiply;
  }
  /* 凌日：从舷窗方向（上方中央）泻入的暖金径向光 */
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
