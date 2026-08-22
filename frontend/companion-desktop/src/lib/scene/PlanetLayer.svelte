<script lang="ts">
  /**
   * PlanetLayer.svelte — 窗外巨行星图层（波次 3b 建，3c/3d 随资产换版重校准布局，
   * 3e 视差升级为头部转动模型，3g 改 canvas 合成动画）
   *
   * 层序：SceneLayer（z 最低，黑洞星空）< PlanetLayer（舷窗外的巨行星，DOM 顺序承接，
   * 不设 z-index）< BridgeLayer（z=1，舰桥内装压住行星四周——行星 physically 在窗外）。
   *
   * 世界观：空间站坐落在左侧巨行星的星环轨道上。planet.webp（2048×1088，纯黑底）
   * 是「从星环上往外看」视角——左上 pale cream-white 星肢巨大贴边、部分出画；
   * 下方沙白/淡金星环平原，环带在左侧 visibly 弯曲抱住星肢（环绕方向修正版），
   * 右端地平线附近被远方金光点亮（朝向黑洞一侧）。地平线实测见下方布局段。
   *   （该段数值随资产换版重测：3c 版地平线 y≈69%，3d 版 y≈57%，勿凭旧值施工。）
   *
   * 波次 3g：canvas 合成动画。底图之上叠两条 X 无缝 tile 流（同向右流）：
   *   - 云带层 limb-clouds-tile.webp（1792×700）：星肢云带自转，慢，115s/周期，
   *     overlay 混合 alpha 0.35，由 planet-mask-limb.webp 约束在星肢区；
   *   - 环带层 ring-bands-tile.webp（1792×640）：环平原金沙带公转，lighter 混合，
   *     由 planet-mask-ring.webp 约束在环平原区。
   *   两张遮罩都是不透明 RGB 灰度图（无 alpha），destination-in 前必须做
   *   亮度→alpha 转换（离屏 getImageData，alpha = max(r,g,b)），否则全图被当作
   *   不透明保留。合成在主 canvas 上以设备像素进行（buffer = 布局尺寸 × min(DPR,2)）。
   * 波次 3i：环带层从匀速传送带改为按行差速流（透视公转读感）——
   *   speed(y) = (W/40)·((y-horizonY)/(H-horizonY))^1.6，horizonY=0.57H（图片地平线实测），
   *   近处（画面底部）快、远处（地平线）趋近静止；alpha 0.5→0.4；
   *   垂直方向三角波重复采样 2 次打散通长直线感。实现见 paintRingFlow()。
   *
   * 布局（波次 3d 按新资产实测重校准，3f 为视差加强加宽，相对 .planet-frame inset:-3% 的原点换算）：
   *   实测：地平线（环平原/太空分界）在图片 y≈57%（x50%–99% 均 0.569–0.571，右侧基本持平）；
   *   环带在左侧弯曲抱住星肢（x30% 处亮区从 y≈22.6% 起）；顶边亮区 x∈[0,25%]（星肢被裁）、
   *   左边 y∈[0,100%] 全亮、右边 y∈[57%,100%] 全亮（max 0.98）、底边亮度 0.49——四边全亮，全要藏。
   *   width:123vw / left:-4.5vw / top:-1.2vh → 视口系下元素占 left:-7.5vw / top:-4.2vh，
   *   宽 123vw、高 116.2vh@1600×900；地平线落在视口 62%（黑洞辉光底部 ~55% 之下，
   *   垂直方向不重叠）。顶/左/右三边出视口余量 38px/120px/248px，
   *   底边沉到视口 112%、由舰桥地板条（~95.6% 起不透明）压住。
   *   注：本资产地平线比上一版高 12%，110vw 已不够把顶边推出视口，故加宽；
   *   3f 为给 0.65° 偏航让出收缩余量再加宽 3vw（120→123vw，居中微调、地平线重锚 62%）。
   *   screen 混合纪律不变：纯黑底前提，本层到场景层之间不得有 stacking context
   *   祖先（fixed/transform/will-change/opacity<1/filter 都会隔离成黑盒）。
   *
   * 景深：中景深度，与 BridgeLayer 同一套头部转动物理模型（弱档，3f 加强到可感知）——
   *   perspective(1400px) + rotateY(-mx*0.65°) 偏航 + rotateX(my*0.45°) 俯仰
   *   + 残余平移 translate3d(-mx*4.5px, -my*2.5px, 0)，符号约定与内装一致；
   *   lerp 0.06/frame；黑洞（含吸积盘）钉死、星野保留原有微视差（3g 降到约 1/3）。
   *   另叠加 ±0.4% 视口宽度的正弦自主漂移（周期 120s，呼吸感）。
   *   视差 transform 与合成 redraw 合在同一个 rAF tick 内完成。
   *   露边核算（123vw，元素 1968×1045px@1600×900，半宽 984/半高 523）：
   *   偏航收缩 = 984·(1−1400/(1400+984·sin0.65°)) ≈ 7.6px（左右边水平内收）；
   *   俯仰收缩 = 523·(1−1400/(1400+523·sin0.45°)) ≈ 1.5px（顶边垂直下移）；
   *   角部合成 z = 984·sin0.65° + 523·sin0.45° ≈ 15.3px → scale 0.9892，
   *   顶边角部下移 ≈ 523·0.0108 ≈ 5.6px。
   *   最坏叠加：顶边 5.6+2.5(平移) ≈ 8.1px < 38px ✓；左边 7.6+4.5+6.4(漂移) ≈ 18.5px < 120px ✓；
   *   右边 18.5px < 248px ✓；底边在地板条下无需核算。0.65°/0.45° 取满不露边。
   *   开发调试：?px=1&py=-1 强制视差到指定归一化位置（./parallax.ts）。
   *
   * Props:
   *   hour —— 舰内时刻 0-24（支持小数），驱动时间线调光：夜航（18:00–04:30）
   *           brightness 降到约 0.7，行照回到 1.0；黎明/暮色坡道与 BridgeLayer/场景层
   *           同一条曲线（timeline.ts 的 sstep/normalizeHour），过渡慢性子 ≥90s。
   *
   * prefers-reduced-motion：禁视差、禁漂移、禁云带/环带流动，只画一帧静态底图（规范 §7.1）。
   */
  import {onMount} from 'svelte';
  import planetWebp from '../../assets/scene/planet.webp';
  import ringBandsTileWebp from '../../assets/scene/ring-bands-tile.webp';
  import limbCloudsTileWebp from '../../assets/scene/limb-clouds-tile.webp';
  import maskLimbWebp from '../../assets/scene/planet-mask-limb.webp';
  import maskRingWebp from '../../assets/scene/planet-mask-ring.webp';
  import {normalizeHour, sstep} from './timeline';
  import {readParallaxOverride} from './parallax';

  let {
    hour = 12,
  }: {
    hour?: number;
  } = $props();

  let planetEl = $state<HTMLCanvasElement>();

  // ---------- 时间线调光（hour → brightness；坡道曲线与 BridgeLayer 完全一致） ----------
  // night：0=行照，14:00–18:00 坡道升入夜航平台，04:30–08:00 黎明坡道回落。
  const lighting = $derived.by(() => {
    const h = normalizeHour(hour);
    const dawn = sstep(4.5, 8.0, h);
    const dusk = sstep(14.0, 18.0, h);
    const night = 1 - dawn * (1 - dusk);
    return {
      brightness: 1 - 0.3 * night, // 夜航 0.70 → 行照 1.0
    };
  });

  onMount(() => {
    const planet = planetEl;
    if (!planet) return;
    const ctx = planet.getContext('2d');
    if (!ctx) return;

    // 复用离屏 canvas：tile 流动 → destination-in 遮罩 → 画回主 canvas
    const off = document.createElement('canvas');
    const offCtx = off.getContext('2d');
    if (!offCtx) return;

    // 合成参数（波次 3i 定稿）：云带慢自转 115s/周期 overlay 0.35（3g 起不变）；
    // 环带改按行差速流（3i）：底部行一个纹理周期 40s、地平线处趋近 0，lighter 0.4。
    // 两者同向右流（x0 随时间增大）。
    const CLOUD_PERIOD_S = 115;
    // 波次 3j：底行 40s→28s（主人"环转快一点"）；flow 0.4→0.6（车道级新纹理扛得住）
    const RING_PERIOD_S = 28;
    const CLOUD_ALPHA = 0.35;
    const RING_ALPHA = 0.6;
    /** 底图静态环弧压暗量——把视觉重量让给流动层 */
    const RING_DIM = 0.42;

    let W = 0;
    let H = 0;
    let baseImg: HTMLImageElement | null = null;
    let cloudImg: HTMLImageElement | null = null;
    let ringImg: HTMLImageElement | null = null;
    let limbMask: HTMLCanvasElement | null = null;
    let ringMask: HTMLCanvasElement | null = null;

    function loadImage(src: string): Promise<HTMLImageElement> {
      return new Promise((resolve, reject) => {
        const img = new Image();
        img.onload = () => resolve(img);
        img.onerror = () => reject(new Error(`image load failed: ${src}`));
        img.src = src;
      });
    }

    // 遮罩是不透明 RGB 灰度图：把亮度写进 alpha，destination-in 才能按灰度软裁
    function maskFromImage(img: HTMLImageElement): HTMLCanvasElement {
      const c = document.createElement('canvas');
      c.width = img.naturalWidth;
      c.height = img.naturalHeight;
      const cctx = c.getContext('2d');
      if (!cctx) return c;
      cctx.drawImage(img, 0, 0);
      const data = cctx.getImageData(0, 0, c.width, c.height);
      const px = data.data;
      for (let i = 0; i < px.length; i += 4) {
        const lum = Math.max(px[i], px[i + 1], px[i + 2]);
        px[i] = 255;
        px[i + 1] = 255;
        px[i + 2] = 255;
        px[i + 3] = lum;
      }
      cctx.putImageData(data, 0, 0);
      return c;
    }

    function resize(): void {
      const rect = planet!.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      W = Math.max(1, Math.round(rect.width * dpr));
      H = Math.max(1, Math.round(rect.height * dpr));
      planet!.width = W;
      planet!.height = H;
      off.width = W;
      off.height = H;
    }

    // 一条 tile 流动层：右流 → 遮罩软裁 → 按混合模式画回主 canvas（云带层用，匀速）
    function paintMaskedFlow(
      tile: HTMLImageElement,
      mask: HTMLCanvasElement,
      periodS: number,
      gco: GlobalCompositeOperation,
      alpha: number,
      tSec: number,
    ): void {
      offCtx!.globalCompositeOperation = 'source-over';
      offCtx!.globalAlpha = 1;
      offCtx!.clearRect(0, 0, W, H);
      // 右流：x0 随时间增大；两份 tile 画在 x0-W 与 x0，恰好铺满 [0, W]
      const x0 = ((tSec * W) / periodS) % W;
      offCtx!.drawImage(tile, x0 - W, 0, W, H);
      offCtx!.drawImage(tile, x0, 0, W, H);
      offCtx!.globalCompositeOperation = 'destination-in';
      offCtx!.drawImage(mask, 0, 0, W, H);
      ctx!.globalCompositeOperation = gco;
      ctx!.globalAlpha = alpha;
      ctx!.drawImage(off, 0, 0);
      ctx!.globalCompositeOperation = 'source-over';
      ctx!.globalAlpha = 1;
    }

    // 波次 3i：环带按行差速流——读感=绕行星公转而非传送带。
    // speed(y) = maxSpeed·((y-horizonY)/(H-horizonY))^1.6，y<horizonY 不画（速度 0）；
    // maxSpeed = W/40（底部行一个纹理周期 40s）。horizonY = 0.57·H（图片地平线 y≈57% 实测，
    // 元素坐标）。行组高 rowH ≈ H/220（dpr1 约 5px、全程约 90-110 组 ×2 次 drawImage，
    // 帧均 ~200 次内）。垂直方向三角波重复采样 2 次（v = tri(2·yn)），打散"通长直线"感；
    // tile 非 Y 无缝，三角波翻转连续无接缝。
    function paintRingFlow(tSec: number): void {
      if (!ringImg || !ringMask) return;
      const TW = ringImg.naturalWidth;
      const TH = ringImg.naturalHeight;
      const horizonY = 0.57 * H;
      const RP = H - horizonY;
      if (RP <= 8) return;
      const maxSpeed = W / RING_PERIOD_S; // device px/s
      const rowH = Math.max(2, Math.round(H / 220));
      const sh = Math.max(1, (rowH * TH * 2) / RP); // 源条带高：匹配三角波垂直缩放
      offCtx!.globalCompositeOperation = 'source-over';
      offCtx!.globalAlpha = 1;
      offCtx!.clearRect(0, 0, W, H);
      for (let y = Math.ceil(horizonY); y < H; y += rowH) {
        const bh = Math.min(rowH, H - y);
        const yn = (y + bh * 0.5 - horizonY) / RP; // 行组中心归一化 0..1
        const speed = maxSpeed * Math.pow(yn, 1.6);
        const x0 = (tSec * speed) % W; // 右流：x0 随时间增大
        const ph = yn * 2;
        const tri = ph < 1 ? ph : 2 - ph;
        const sy = Math.min(TH - sh, tri * (TH - sh));
        // 目标行组 [y, y+bh)：两份 tile 条带画在 x0-W 与 x0，铺满 [0, W]
        offCtx!.drawImage(ringImg, 0, sy, TW, sh, x0 - W, y, W, bh);
        offCtx!.drawImage(ringImg, 0, sy, TW, sh, x0, y, W, bh);
      }
      offCtx!.globalCompositeOperation = 'destination-in';
      offCtx!.drawImage(ringMask, 0, 0, W, H);
      ctx!.globalCompositeOperation = 'lighter';
      ctx!.globalAlpha = RING_ALPHA;
      ctx!.drawImage(off, 0, 0);
      ctx!.globalCompositeOperation = 'source-over';
      ctx!.globalAlpha = 1;
    }

    function compose(tSec: number): void {
      if (W === 0 || H === 0) return;
      ctx!.globalCompositeOperation = 'source-over';
      ctx!.globalAlpha = 1;
      ctx!.clearRect(0, 0, W, H);
      // 开发工具：?ringonly=1 只画环带层（黑底），用于位移场测量
      if (typeof location !== 'undefined' && new URLSearchParams(location.search).has('ringonly')) {
        ctx!.fillStyle = '#000';
        ctx!.fillRect(0, 0, W, H);
        paintRingFlow(tSec);
        return;
      }
      if (!baseImg) return;
      ctx!.drawImage(baseImg, 0, 0, W, H);
      // 波次 3j：环区静态车道压暗——把视觉重量让给流动层（底图大弧线不动、
      // 只有细线叠加层在动 = 主人"转的不对"的结构根因）。ringMask 软边，压暗无硬界。
      if (ringMask) {
        offCtx!.globalCompositeOperation = 'source-over';
        offCtx!.globalAlpha = 1;
        offCtx!.clearRect(0, 0, W, H);
        offCtx!.fillStyle = '#000';
        offCtx!.fillRect(0, 0, W, H);
        offCtx!.globalCompositeOperation = 'destination-in';
        offCtx!.drawImage(ringMask, 0, 0, W, H);
        ctx!.globalAlpha = RING_DIM;
        ctx!.drawImage(off, 0, 0);
        ctx!.globalAlpha = 1;
      }
      if (cloudImg && limbMask) {
        paintMaskedFlow(cloudImg, limbMask, CLOUD_PERIOD_S, 'overlay', CLOUD_ALPHA, tSec);
      }
      paintRingFlow(tSec);
    }

    // reduced-motion：视差/漂移/流动全停，只留静态底图一帧（规范 §7.1）
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    // 开发覆写 ?px=&py=：两轴齐备且有限时锁定视差目标，不跟随鼠标
    const override = readParallaxOverride();
    // 开发工具：?t0=<秒> 冻结动画时钟，供 headless 帧差验证（与 blackhole.ts 同名参数）
    const t0Raw = parseFloat(new URLSearchParams(location.search).get('t0') ?? '');
    const t0Freeze = Number.isFinite(t0Raw) ? t0Raw : null;

    let mx = 0;
    let my = 0;
    let mxTarget = 0;
    let myTarget = 0;
    let raf: number | null = null;
    const DRIFT_PERIOD_S = 120;

    function tick(nowMs: number): void {
      raf = null;
      // 鼠标视差：lerp 0.06/frame 平滑趋近目标（与 BridgeLayer 同手感）
      mx += (mxTarget - mx) * 0.06;
      my += (myTarget - my) * 0.06;
      const tSec = t0Freeze ?? nowMs / 1000;
      // 缓慢自主漂移：±0.4% 视口宽度，周期 120s，呼吸感
      const driftX = Math.sin((tSec * Math.PI * 2) / DRIFT_PERIOD_S) * window.innerWidth * 0.004;
      // 波次 3f 中景弱档加强到可感知：偏航 -mx*0.65° / 俯仰 my*0.45°
      // + 残余平移 (-mx*4.5px, -my*2.5px) + 漂移。符号与 BridgeLayer 同一约定。
      // transform 仍只写在混合元素自身（screen 混合纪律：任何祖先带
      // transform/stacking context 都会把混合隔离成黑盒；元素自身的
      // perspective/rotate 只隔离其不存在的子节点，混合穿透不受影响）。
      // 露边核算见文件头（最坏叠加 18.5px < 最紧顶边余量 38px）。
      planet!.style.transform =
        `perspective(1400px) rotateX(${(my * 0.45).toFixed(3)}deg) ` +
        `rotateY(${(-mx * 0.65).toFixed(3)}deg) ` +
        `translate3d(${(-mx * 4.5 + driftX).toFixed(2)}px, ${(-my * 2.5).toFixed(2)}px, 0)`;
      // 合成 redraw 与视差同 tick
      compose(tSec);
      raf = requestAnimationFrame(tick);
    }

    function onMove(e: MouseEvent): void {
      // 中景深度：转动/平移都比内装（1.0°/±4px）弱，比钉死的黑洞/星野多一点
      mxTarget = Math.min(1, Math.max(-1, (e.clientX / window.innerWidth) * 2 - 1));
      myTarget = Math.min(1, Math.max(-1, (e.clientY / window.innerHeight) * 2 - 1));
    }

    function onLeave(): void {
      mxTarget = 0;
      myTarget = 0;
    }

    function onResize(): void {
      resize();
      // buffer 重置后必须重画：动画模式下一帧也会重画，这里兜底静态/reduced 场景
      compose(performance.now() / 1000);
    }

    resize();
    window.addEventListener('resize', onResize);

    if (reduceMotion) {
      // 只画静态底图，不起 rAF、不加鼠标监听
      loadImage(planetWebp)
        .then((img) => {
          baseImg = img;
          compose(0);
        })
        .catch(() => {});
      return () => {
        window.removeEventListener('resize', onResize);
      };
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

    // 底图先到先画（兜底），tile/遮罩就绪后各层自动加入合成
    loadImage(planetWebp)
      .then((img) => {
        baseImg = img;
      })
      .catch(() => {});
    Promise.all([
      loadImage(limbCloudsTileWebp),
      loadImage(ringBandsTileWebp),
      loadImage(maskLimbWebp),
      loadImage(maskRingWebp),
    ])
      .then(([cloud, ring, mLimb, mRing]) => {
        cloudImg = cloud;
        ringImg = ring;
        limbMask = maskFromImage(mLimb);
        ringMask = maskFromImage(mRing);
      })
      .catch(() => {});

    return () => {
      window.removeEventListener('resize', onResize);
      if (!override) {
        window.removeEventListener('mousemove', onMove);
        document.documentElement.removeEventListener('mouseleave', onLeave);
        window.removeEventListener('blur', onLeave);
      }
      if (raf !== null) cancelAnimationFrame(raf);
    };
  });
</script>

<div class="planet-layer" aria-hidden="true">
  <div class="planet-frame">
    <canvas
      class="planet"
      bind:this={planetEl}
      style:filter={`brightness(${lighting.brightness.toFixed(3)})`}
    ></canvas>
  </div>
</div>

<style>
  /* 中景层：SceneLayer 之上（DOM 顺序承接，不设 z-index）、BridgeLayer（z=1）之下；
     不截获任何指针事件（点黑洞 = 临渊机位的命中路径不被遮挡）。
     注意：必须用 absolute 而非 fixed——本层到场景层之间不得存在任何 stacking context
     祖先（fixed/transform/will-change/opacity<1/filter 都会创建），否则 .planet 的
     screen 混合会被隔离在空背景上，黑底显形为方框。 */
  .planet-layer {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }
  /* 定位容器：与 BridgeLayer .bridge-frame 同样 inset:-3%，保留视差出血余量。
     本元素不挂 transform（视差写在 .planet 上），避免成为隔离组。 */
  .planet-frame {
    position: absolute;
    inset: -3%;
  }
  /* 行星系统（波次 3g 改 canvas 合成，几何与 3f 一致）：「从星环上往外看」——
     左上星肢巨大贴边出画，环带左侧弯曲抱星肢，下方星环平原。
     相对 .planet-frame（inset:-3%）定位；换算到视口系：元素 left:-7.5vw /
     top:-4.2vh / 宽 123vw / 高 116.2vh@1600×900，图片地平线（y≈57%）落在视口 62%，
     在黑洞辉光底部（~55%）之下、垂直不重叠。四条亮边全藏：顶/左/右边出视口
     （余量 38/120/248px，露边核算见文件头），底边沉到视口 112% 被舰桥地板条
     （~95.6% 起不透明）压住。
     screen 混合：canvas 内纯黑区直接融入星空，无方框边缘；元素不得带非纯黑背景/边框。
     transform 只作用元素自身（无子节点可隔离），混合仍穿透到场景层。 */
  .planet {
    position: absolute;
    left: -4.5vw;
    top: -1.2vh;
    width: 123vw;
    aspect-ratio: 2048 / 1088;
    display: block;
    mix-blend-mode: screen;
    /* 照明过渡慢性子（规范 §3.3，令牌 motion.timeline ≥60s，取默认 90s） */
    transition: filter var(--ap-dur-timeline, 90s) linear;
  }
</style>
