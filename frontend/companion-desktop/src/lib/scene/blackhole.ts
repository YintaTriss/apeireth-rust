/**
 * blackhole.ts — 黑洞场景纯渲染引擎（框架无关 TS 模块，零依赖）
 *
 * 移植自 frontend/design-preview/index.html v0.2（已验收原型），渲染数值与原型逐行一致：
 *   - WebGL2 shader：吸积盘 / 透镜弧 / 光子环 / 三层视差星野 / 银河带
 *   - Canvas 2D 降级（WebGL2 不可用时自动切换，不黑屏）
 *   - 时间线照明四 uniform：uAmbStr / uEclipse / uTlBright / uTlGold（曲线见 ./timeline.ts）
 *   - presence 状态机：quiet / thinking / speaking（指数趋近，无硬切换）
 *   - 机位五元组 + 轨道式鼠标视差 + 三组正弦自动漂移
 *
 * 性能纪律（规范 §7.3，✅ 与原型一致）：
 *   DPR ≤ 2；document.hidden 停 rAF；prefers-reduced-motion → 静态单帧 + 锁定远眺机位；
 *   帧时长 clamp 50ms；incl clamp [0.06, 1.2]。
 *
 * 与原型的刻意差异（均为应用化适配，不改变渲染数值）：
 *   1. 原型的调试 URL 参数（?force2d/?state/?cam/?mx/?my/?hour）改为构造选项与方法调用；
 *   2. 原型的 FPS 状态行/DOM UI 不属于本模块；
 *   3. 胶片颗粒与暗角是全息/后期层职责，不在此实现；
 *   4. 原型 fallback 路径为纯静态；本模块在 hour=null（本地时钟）时为静态路径（fallback 或
 *      reduced-motion）增加 60s 节律重绘，让时间线照明跟随真实时间；
 *   5. destroy() 时主动 loseContext，便于组件卸载回收 GPU 资源。
 */

import {
  CAMERA_DRIFT,
  CAMERA_INCL_CLAMP,
  CAMERA_PARALLAX,
  CAMERA_PRESETS,
  MOTION,
  PERFORMANCE,
  PRESENCE_STATES,
  SCENE_LAYOUT,
  STAR_LAYERS,
  type CameraPreset,
  type SceneMode,
} from './tokens';
import {localClockHour, normalizeHour, timelineParams} from './timeline';

export type {SceneMode} from './tokens';

/** SceneLayer 的 presence prop 形状（与将来的 src/lib/presence.ts presenceStore 订阅值对齐） */
export interface PresenceInput {
  p: number;
  a: number;
  d: number;
  mode: SceneMode;
}

/** PAD 偏移（v1 简化映射；d 暂存不消费 —— warmth 需改 shader 新增 uniform，列 v1.1，规范 §4.1/B-2） */
export interface PadBias {
  p: number;
  a: number;
  d: number;
}

/** 黑洞视区圆（CSS px，供命中测试/悬停指针） */
export interface HoleCircle {
  x: number;
  y: number;
  r: number;
}

/** Canvas 2D 降级路径的 DOM 呼吸光位置（CSS px） */
export interface FallbackPulse {
  x: number;
  y: number;
  diameter: number;
}

/**
 * Canvas 2D 降级路径专用 DOM 视觉（WebGL 路径由 shader uniform 承担，组件无需处理）：
 * 行照环境蓝叠加层 / 凌日压暗层 / 说话呼吸光。数值与原型 #amb-overlay/#eclipse-overlay/#pulse 一致。
 */
export interface FallbackVisuals {
  pulse: FallbackPulse | null;
  ambOpacity: number;
  eclipseOpacity: number;
}

export interface BlackholeSceneOptions {
  /** 是否响应鼠标轨道视差（页面层打开时调用方关掉）；默认 true */
  interactive?: boolean;
  /** 默认读 prefers-reduced-motion；显式传入可覆盖 */
  reduceMotion?: boolean;
  /** 强制 Canvas 2D 路径（调试/验收用，等价原型 ?force2d=1） */
  forceFallback?: boolean;
  /** 降级路径视觉变更回调（每次 fallback 重绘后触发） */
  onFallbackVisuals?: (visuals: FallbackVisuals) => void;
}

interface EffectiveCamera {
  /** 视角倾角（含鼠标视差）——波次 3f 起只驱动星野 */
  incl: number;
  zoom: number;
  /** 视角方位角（含鼠标视差）——波次 3f 起只驱动星野 */
  azim: number;
  /** 盘面姿态倾角（机位预设+自主漂移，不含鼠标视差）——盘面/黑洞对鼠标零响应 */
  diskIncl: number;
  /** 盘面姿态方位角（同上） */
  diskAzim: number;
  cx: number;
  cy: number;
}

interface FallbackStar {
  x: number;
  y: number;
  r: number;
  a: number;
  k: number;
  c: string;
}

const TAU = Math.PI * 2;
/** 静态路径（fallback / reduced-motion）跟随本地时钟的重绘间隔 */
const CLOCK_REDRAW_MS = 60_000;

/* ================================================================
 * GLSL —— 原型 VERT/FRAG 已验收资产（光学数值禁止改动）。
 * 波次 3f 唯一结构性变更：新增 uDiskAzim/uDiskIncl 两个 uniform，
 * 把「盘面姿态」与「星野视角」从共用的 uAzim/uIncl 拆成两个通道——
 * 盘面（含吸积盘外环/光环比/晕辉）对鼠标零响应，星野三层视差保留。
 * 所有光学计算数值与原型一致，仅角度来源分流。
 * ================================================================ */

const VERT = `#version 300 es
layout(location=0) in vec2 aPos;
void main(){ gl_Position = vec4(aPos, 0.0, 1.0); }`;

// uniform 注释（原型 JS 侧注释，移入此处备查）：
//   uAmbStr   行照环境蓝光强度 0..1
//   uEclipse  凌日日食 0..1
//   uTlBright 时间线亮度增益
//   uTlGold   时间线金色占比 0..1
const FRAG = `#version 300 es
precision highp float;
uniform vec2  uRes;
uniform vec2  uCenter;
uniform float uScale;
uniform float uTime;
uniform float uRot;
uniform float uBright;
uniform float uTurb;
uniform float uPulse;
uniform float uIncl;
uniform float uAzim;
// 波次 3f：盘面专用姿态角（不含鼠标视差）；uAzim/uIncl 退化为星野视角专用
uniform float uDiskAzim;
uniform float uDiskIncl;
uniform float uAmbStr;
uniform float uEclipse;
uniform float uTlBright;
uniform float uTlGold;
out vec4 fragColor;

float hash21(vec2 p){
    p = fract(p * vec2(234.34, 435.345));
    p += dot(p, p + 34.23);
    return fract(p.x * p.y);
}
float vnoise(vec2 p){
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash21(i);
    float b = hash21(i + vec2(1.0, 0.0));
    float c = hash21(i + vec2(0.0, 1.0));
    float d = hash21(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}
float fbm(vec2 p){
    float v = 0.0;
    float a = 0.5;
    for(int i = 0; i < 5; i++){
        v += a * vnoise(p);
        p = p * 2.03 + vec2(17.3, 9.1);
        a *= 0.5;
    }
    return v;
}

vec3 diskRadiance(float r, float phi, float t){
    if(r > 5.6) return vec3(0.0);
    float rIn  = 1.45;
    float rOut = 5.0;
    float band = smoothstep(rIn, rIn + 0.40, r) * (1.0 - smoothstep(rIn + 1.3, rOut, r));
    // 波次 3i：时间驱动较差自转。Ω(r) = 0.70·(1.45/r)^1.5 rad/s 加在 pa=phi·2+pr 的 pr 上，
    // 图案角速度 = Ω/2（二重对称）：r=1.45 内亮带 -20°/s（6s≈-120°）、r=2.2 ≈-10.7°/s
    // （6s≈-64°）、r=3.0 ≈-6.8°/s（6s≈-41°）、r=4.5 外亮带 ≈-3.7°/s（6s≈-22°）。
    // uRot（presence 驱动）保留不动；t 即 uTime（引擎 tSec，秒，dt clamp 50ms）。
    // （3g 首版 0.314 太弱 + 噪声各向异性反向 → 主人实测判"不转"，本版修正。）
    float pr = uRot * (2.6 / (r * r * 0.5 + 0.5)) + t * (0.70 * pow(1.45 / r, 1.5));
    float pa = phi * 2.0 + pr;
    // 波次 3i 关键修正：噪声频率各向异性翻转——原 角向1.3/径向3.6 产生同心环沟槽，
    // 沟槽原地滑动视觉上等于不转（主人"不转"判定的 shader 层 root cause）。
    // 改 角向2.6/径向0.9：切向锐利、径向拉长的辐条涡纹（每圈 ~33 个 ~11° 特征），
    // 绕转直接可读；径向沸动物理速率不变（0.20/3.6 = 0.05/0.9 = 0.056 r-单位/s）。
    // 波次 3j 沸腾刹车：径向沸腾 0.056 r-单位/s（6s 整带换新 1.5 次）曾让纹理原地 churn、
    // 公转读感被沸腾淹没（"在转但看起来像冒烟"的感知根因）。降到 ~0.009：
    // 6s 只蠕动 5% 带宽，公转（内带 101°/6s）成为唯一主导运动。
    float n  = fbm(vec2(pa * 2.6, r * 0.9 - t * 0.008));
    float n2 = fbm(vec2(pa * 8.4 + 7.7, r * 1.8 - t * 0.015));
    // 波次 3j：共转团块——低频角向结构随同一 Ω(r) 较差自转。
    // 修"不转"感知的根因：细纹转得再快肉眼也抓不住，可追踪的大尺度亮团才读作"在转"。
    // v2 校准：团块要"硬边结块"而非"慢变云"——窄过渡带 + 2.9 倍提亮，暗隙更深，
    // 电影条上能逐帧追踪同一个结块的角位移。
    float cl = fbm(vec2(pa * 1.15 + 3.3, r * 0.55 - t * 0.006));
    float clump = smoothstep(0.50, 0.60, cl);
    // 波次 3i：quiet 档涡纹对比 ±15%→±40%（0.30n → 0.90n-0.27，quiet 均值仍 ~0.72
    // 不整体增亮）；uTurb 高湍流路径不动，Doppler 明暗不对称保留（3j 收敛动态范围）。
    float streak = 0.50 + (0.85 * n + 0.60 * n2) * uTurb + (0.90 * n - 0.27) * (1.0 - uTurb);
    streak *= (0.40 + 0.60 * uTurb) + (1.80 - 0.60 * uTurb) * clump;  // quiet 档：结块主导
    float hr = clamp((r - rIn) / (rOut - rIn), 0.0, 1.0);
    vec3 whiteHot = vec3(1.000, 0.950, 0.820);
    vec3 gold     = vec3(1.000, 0.824, 0.478);
    vec3 amber    = vec3(0.910, 0.639, 0.239);
    vec3 c = mix(gold, amber, smoothstep(0.30, 1.0, hr));
    c = mix(whiteHot, c, smoothstep(0.02, 0.22, hr));
    // 波次 3j：dop 0.95→0.72（明暗比 38:1→6:1）——保留单侧增亮的卡冈图雅读感，
    // 但不再把亮侧半盘推进色调映射饱和区（饱和=纹理压平=旋转不可见的机制根因）。
    float dop = 1.0 - 0.72 * cos(phi);
    c = mix(c, vec3(1.0, 0.955, 0.830), clamp((dop - 1.0) * 0.45, 0.0, 1.0));
    // 行照时金色占比降低：盘面去饱和、略偏冷，亮度由 uTlBright 补偿
    float lum = dot(c, vec3(0.3333));
    c = mix(c, lum * vec3(0.92, 0.99, 1.14), (1.0 - uTlGold) * 0.45);
    float rim = exp(-pow((r - 1.52) * 5.0, 2.0));
    // 波次 3j：远眺机位实拍校准——quiet.bright=0.32 的设计值标定于原型满屏黑洞；
    // 0.6 倍远眺后盘面像素面积缩到 1/3，1.6 的乘子整体沉入噪底（"不转"另一根因：
    // 不是没转，是暗到看不见）。0.72→1.9 后 quiet 有效亮度 0.61，团块纹理浮出天空底。
    vec3 radiance = c * band * streak * dop * uBright * uTlBright * 1.9;
    radiance += vec3(1.0, 0.96, 0.87) * rim * dop * (0.6 + 0.7 * streak) * uBright * uTlBright * 1.1;
    return radiance;
}

// one depth layer of the starfield; voff = view-angle offset (parallax scaled by k)
vec3 starLayer(vec2 suv, vec2 voff, float k, float scale, float thr, float sizeMul, float boost){
    vec2 g = (suv + voff * k) * scale;
    vec2 id = floor(g);
    float h = hash21(id);
    if(h <= thr) return vec3(0.0);
    vec2 jit = vec2(hash21(id + 7.13), hash21(id + 3.71)) - 0.5;
    float sd = length(fract(g) - 0.5 - jit * 0.7);
    float mag = (h - thr) / (1.0 - thr);
    float rad = 0.14 * sizeMul * (0.6 + 0.8 * mag);
    float s = 1.0 - smoothstep(0.0, rad, sd);
    float hc = hash21(id + 17.7);
    vec3 tint = vec3(0.95, 0.85, 0.62);
    if(hc < 0.12) tint = vec3(0.72, 0.80, 1.00);
    else if(hc < 0.55) tint = vec3(0.98, 0.93, 0.80);
    return tint * s * (0.08 + 0.55 * mag) * boost;
}

void main(){
    vec2 frag = gl_FragCoord.xy;
    vec2 p = (frag - uCenter) / uScale;
    // 波次 3f 视差通道拆分：盘面/光环比/晕辉的局部坐标系用 uDiskAzim/uDiskIncl
    // （机位预设+自主漂移，不含鼠标视差——盘面与黑洞本体对鼠标零响应）；
    // 星野 voff 仍用 uAzim/uIncl（含鼠标视差，星星保持三层视差与旋转）。
    float ca = cos(uDiskAzim);
    float sa = sin(uDiskAzim);
    p = mat2(ca, -sa, sa, ca) * p;
    float incl = uDiskIncl;
    float rp = length(p);
    float t = uTime;
    float hole = smoothstep(0.985, 1.045, rp);
    float pulseWave = 0.5 + 0.5 * sin(t * 1.9);

    vec3 sky = vec3(0.027, 0.027, 0.047);

    // 3D starfield: milky-way band + three depth layers with view-angle parallax.
    // 视角右转(azim+) → 星野向左流；近层位移 > 远层。黑洞本体与盘面均不动。
    {
        vec2 suv = frag / uRes.y;
        vec2 voff = vec2(uAzim, (uIncl - 0.30) * 0.8);
        vec2 bsuv = suv + voff * 0.45;
        float bandAxis = dot(bsuv, vec2(-0.485, 0.874));
        float bd = (bandAxis - 0.60) * 2.2;
        float mw = exp(-bd * bd);
        float mwTex = fbm(vec2(bsuv.x * 2.2 + bsuv.y * 1.1, bandAxis * 7.0));
        float neb = fbm(bsuv * 3.5 + 4.7);
        sky += vec3(0.90, 0.80, 0.58) * mw * (0.20 + 0.55 * mwTex + 0.35 * neb) * 0.16;
        float starBoost = (1.0 + 2.2 * mw) * (1.0 - 0.55 * uAmbStr);
        sky += starLayer(suv, voff, 0.50, 230.0, 0.9960, 0.80, starBoost);
        sky += starLayer(suv, voff, 0.90, 150.0, 0.9955, 1.15, starBoost);
        sky += starLayer(suv, voff, 1.40,  95.0, 0.9965, 1.70, starBoost);
    }

    // 行照：行星大气反照的环境蓝（顶部更强）；凌日：环境压暗
    sky += vec3(0.42, 0.58, 0.92) * uAmbStr * (0.10 + 0.16 * (frag.y / uRes.y));
    sky *= 1.0 + 0.18 * uAmbStr;
    sky *= 1.0 - 0.80 * uEclipse;

    vec3 col = sky;

    // gold halo / bloom skirts
    float outer = max(rp - 1.0, 0.0);
    col += vec3(1.00, 0.78, 0.38) * 0.060 * exp(-outer * 0.85) * (0.3 + uBright) * (1.0 - 0.60 * uEclipse);
    col += vec3(0.92, 0.62, 0.24) * 0.030 * exp(-outer * 0.26) * (0.3 + uBright) * (1.0 - 0.60 * uEclipse);

    // back half of the disk, bent over the top (lensing approximation)
    {
        float back = smoothstep(-0.30, 0.55, p.y);
        float lift = 1.15 * exp(-outer * 0.85);
        vec2 pa2 = vec2(p.x, (p.y - lift * back) / incl);
        vec3 arcTop = diskRadiance(length(pa2), atan(pa2.y, pa2.x), t) * back;
        vec2 pb = vec2(p.x, (p.y + lift * back * 0.55) / incl);
        vec3 arcBot = diskRadiance(length(pb), atan(pb.y, pb.x), t) * back * 0.38;
        col += (arcTop + arcBot) * hole;
    }

    // near side passes in front of the horizon
    {
        float front = 1.0 - smoothstep(-0.18, 0.30, p.y);
        vec2 pf = vec2(p.x, p.y / incl);
        col += diskRadiance(length(pf), atan(pf.y, pf.x), t) * front * hole;
    }

    // filmic-ish tonemap, then force the horizon to pure black
    col = 1.0 - exp(-col * 1.55 * (1.0 + 0.15 * uAmbStr));
    col *= hole;

    // sharp photon ring + faint gold secondary ring (凌日时成为唯一光源，增强)
    float ringI = exp(-pow((rp - 1.045) * 42.0, 2.0));
    col += vec3(1.0, 0.96, 0.86) * ringI * (0.60 + 0.30 * uBright + 0.50 * uPulse * pulseWave) * (1.0 + 0.8 * uEclipse);
    float ring2 = exp(-pow((rp - 1.14) * 13.0, 2.0));
    col += vec3(1.0, 0.82, 0.50) * ring2 * 0.16 * uBright;

    // breathing central light point (speaking state)
    col += vec3(1.0, 0.95, 0.85) * exp(-rp * 2.4) * uPulse * (0.22 + 0.38 * pulseWave) * 0.55;

    fragColor = vec4(col, 1.0);
}`;

const UNIFORM_NAMES = [
  'uRes',
  'uCenter',
  'uScale',
  'uTime',
  'uRot',
  'uBright',
  'uTurb',
  'uPulse',
  'uIncl',
  'uAzim',
  'uDiskAzim',
  'uDiskIncl',
  'uAmbStr',
  'uEclipse',
  'uTlBright',
  'uTlGold',
] as const;

/* ================================================================ */

function easeInOutCubic(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

function round3(v: number): number {
  return Math.round(v * 1000) / 1000;
}

export class BlackholeScene {
  private canvas: HTMLCanvasElement;
  private readonly opts: BlackholeSceneOptions;
  private readonly reduceMotion: boolean;
  private destroyed = false;

  /** true → Canvas 2D 降级路径 */
  readonly fallback: boolean;

  /* ---- presence 状态机 ---- */
  private stateName: SceneMode = 'quiet';
  private padBias: PadBias = {p: 0, a: 0, d: 0};
  private cur = {speed: PRESENCE_STATES.quiet.speed, bright: PRESENCE_STATES.quiet.bright, turb: PRESENCE_STATES.quiet.turb, pulse: PRESENCE_STATES.quiet.pulse};
  private rot = 0.0;
  private tSec = 0.0;

  /* 开发工具：?t0=<秒> 冻结动画时钟（tSec=t0、rot=0）——
     headless virtual-time 下 WebGL 只呈现早期帧，借此截取任意时刻盘面状态做帧差验证。 */
  private debugT0: number | null =
    typeof location !== 'undefined'
      ? (() => {
          const v = parseFloat(new URLSearchParams(location.search).get('t0') ?? '');
          return Number.isFinite(v) ? v : null;
        })()
      : null;

  /* ---- 机位 ---- */
  private camIdx = 0;
  private camCur: {incl: number; zoom: number; azim: number; ox: number; oy: number};
  private camFrom: {incl: number; zoom: number; azim: number; ox: number; oy: number};
  private camTween = 1;

  /* ---- 布局（CSS px） ---- */
  private cssW = 0;
  private cssH = 0;
  private baseCx = 0;
  private baseCy = 0;
  private holeR = 0;
  private dpr = 1;
  private rectLeft = 0;
  private rectTop = 0;
  private lastHole: HoleCircle = {x: 0, y: 0, r: 0};

  /* ---- 鼠标轨道视差 ---- */
  private interactive: boolean;
  private mouseTX = 0;
  private mouseTY = 0;
  private mouseX = 0;
  private mouseY = 0;

  /* ---- 时间线 ---- */
  private hour: number | null = null;
  private clockTimer: number | null = null;

  /* ---- WebGL ---- */
  private gl: WebGL2RenderingContext | null = null;
  private uni: Record<string, WebGLUniformLocation | null> = {};
  private rafId: number | null = null;
  private lastT = 0;

  /* ---- Canvas 2D 降级 ---- */
  private ctx2d: CanvasRenderingContext2D | null = null;
  private stars: FallbackStar[] = [];

  constructor(canvas: HTMLCanvasElement, options: BlackholeSceneOptions = {}) {
    this.opts = options;
    this.canvas = canvas;
    this.reduceMotion =
      options.reduceMotion ??
      (typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches);
    this.interactive = options.interactive ?? true;

    const initialCam = CAMERA_PRESETS[0];
    this.camCur = {incl: initialCam.incl, zoom: initialCam.zoom, azim: initialCam.azim, ox: initialCam.ox, oy: initialCam.oy};
    this.camFrom = {...this.camCur};

    this.layout();
    this.fallback = options.forceFallback === true || !this.initGL();

    if (!this.fallback) {
      this.resizeGL();
      if (this.reduceMotion) {
        this.renderOnceGL();
      } else {
        this.startLoop();
        document.addEventListener('visibilitychange', this.visibilityHandler);
      }
      window.addEventListener('resize', this.resizeHandler);
    } else {
      if (options.forceFallback !== true) {
        console.warn('[scene] WebGL2 不可用，已降级 Canvas 2D 静态渲染');
      }
      this.ctx2d = this.canvas.getContext('2d');
      if (!this.ctx2d) {
        // canvas 被失败的 WebGL2 尝试锁定：换一个新节点（原型同款处理）
        const fresh = this.canvas.cloneNode(false) as HTMLCanvasElement;
        this.canvas.parentNode?.replaceChild(fresh, this.canvas);
        this.canvas = fresh;
        this.ctx2d = this.canvas.getContext('2d');
      }
      if (this.ctx2d) {
        this.resizeFallback();
        window.addEventListener('resize', this.resizeHandler);
      }
      // 连 2D context 都拿不到时：组件根节点保留 --void 底色，不黑屏
    }

    window.addEventListener('mousemove', this.mouseHandler);
    this.updateHoleCircleFromPreset();
    this.updateClockTimer();
  }

  /* ================================================================
   * 公开 API
   * ================================================================ */

  /** presence 状态机切换（quiet/thinking/speaking） */
  setMode(mode: SceneMode): void {
    if (this.destroyed || this.stateName === mode) return;
    this.stateName = mode;
    this.refreshStatic();
  }

  /**
   * PAD → 视觉的 v1 简化映射（🟡 proposal，数值 TODO 待实拍校准，规范 §4.1 / 附录 B-2）：
   *   bright += p * 0.15
   *   speed  += a * 0.4
   * d 暂存不消费（warmth 列 v1.1，需改 shader）。
   */
  setPadBias(bias: PadBias): void {
    if (this.destroyed) return;
    this.padBias = {p: bias.p, a: bias.a, d: bias.d};
    this.refreshStatic();
  }

  /** 舰内时刻（0-24，可为小数）；null → 跟随本地时钟 */
  setHour(hour: number | null): void {
    if (this.destroyed) return;
    this.hour = hour === null ? null : normalizeHour(hour);
    this.updateClockTimer();
    this.refreshStatic();
  }

  /** 鼠标轨道视差开关；关闭时视角平滑回中 */
  setInteractive(on: boolean): void {
    this.interactive = on;
    if (!on) {
      this.mouseTX = 0;
      this.mouseTY = 0;
    }
  }

  /** 机位切换（reduced-motion 时锁定远眺，与原型一致） */
  setCamera(index: number): void {
    if (this.destroyed || index < 0 || index >= CAMERA_PRESETS.length) return;
    const i = this.reduceMotion ? 0 : index;
    this.camIdx = i;
    this.camFrom = {...this.camCur};
    this.camTween = this.reduceMotion ? 1 : 0;
    this.refreshStatic();
    this.updateHoleCircleFromPreset();
  }

  getCameraIndex(): number {
    return this.camIdx;
  }

  /** 当前黑洞视区圆（CSS px，基于最近一帧的有效机位） */
  holeScreenCircle(): HoleCircle {
    return {...this.lastHole};
  }

  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.stopLoop();
    if (this.clockTimer !== null) {
      clearInterval(this.clockTimer);
      this.clockTimer = null;
    }
    window.removeEventListener('resize', this.resizeHandler);
    window.removeEventListener('mousemove', this.mouseHandler);
    document.removeEventListener('visibilitychange', this.visibilityHandler);
    if (this.gl) {
      const ext = this.gl.getExtension('WEBGL_lose_context');
      if (ext) ext.loseContext();
      this.gl = null;
    }
  }

  /* ================================================================
   * 布局 / 有效机位
   * ================================================================ */

  private layout(): void {
    const rect = this.canvas.getBoundingClientRect();
    this.cssW = Math.max(1, rect.width || window.innerWidth);
    this.cssH = Math.max(1, rect.height || window.innerHeight);
    this.rectLeft = rect.left;
    this.rectTop = rect.top;
    this.dpr = Math.min(window.devicePixelRatio || 1, PERFORMANCE.dprMax);
    const L = SCENE_LAYOUT;
    if (this.cssW <= L.breakpointPx) {
      this.baseCx = this.cssW * L.mobile.cxRatio;
      this.baseCy = this.cssH * L.mobile.cyRatio;
      this.holeR = Math.min(this.cssW, this.cssH) * L.mobile.holeRadiusRatio;
    } else {
      this.baseCx = this.cssW * L.desktop.cxRatio;
      this.baseCy = this.cssH * L.desktop.cyRatio;
      this.holeR = Math.min(this.cssW, this.cssH) * L.desktop.holeRadiusRatio;
    }
  }

  /**
   * 每帧计算机位：预设插值 + 自动漂移 + 鼠标轨道视角。
   * 黑洞构图中心永远钉死（ox/oy 仅来自机位预设）。
   * 波次 3f 视差通道拆分：鼠标轨道角（parAz/parIncl）只进 incl/azim（星野视角，
   * 星星保持三层视差与旋转）；diskIncl/diskAzim 只含预设插值 + 自主漂移——
   * 盘面姿态、光环比、晕辉与黑洞本体对鼠标零响应。
   */
  private effectiveCamera(dt: number, t: number): EffectiveCamera {
    if (this.camTween < 1) {
      this.camTween = Math.min(1, this.camTween + dt / MOTION.cameraTweenSec);
      const e = easeInOutCubic(this.camTween);
      const tgt = CAMERA_PRESETS[this.camIdx];
      this.camCur.incl = this.camFrom.incl + (tgt.incl - this.camFrom.incl) * e;
      this.camCur.zoom = this.camFrom.zoom + (tgt.zoom - this.camFrom.zoom) * e;
      this.camCur.azim = this.camFrom.azim + (tgt.azim - this.camFrom.azim) * e;
      this.camCur.ox = this.camFrom.ox + (tgt.ox - this.camFrom.ox) * e;
      this.camCur.oy = this.camFrom.oy + (tgt.oy - this.camFrom.oy) * e;
    }
    let driftAz = 0;
    let driftIncl = 0;
    let driftZoom = 1;
    let parAz = 0;
    let parIncl = 0;
    if (!this.reduceMotion) {
      driftAz = CAMERA_DRIFT.azimuth.amplitude * Math.sin((t * TAU) / CAMERA_DRIFT.azimuth.periodSec + CAMERA_DRIFT.azimuth.phase);
      driftIncl = CAMERA_DRIFT.inclination.amplitude * Math.sin((t * TAU) / CAMERA_DRIFT.inclination.periodSec + CAMERA_DRIFT.inclination.phase);
      driftZoom = 1 + CAMERA_DRIFT.zoom.amplitude * Math.sin((t * TAU) / CAMERA_DRIFT.zoom.periodSec + CAMERA_DRIFT.zoom.phase);
      const k = Math.min(1, dt * CAMERA_PARALLAX.smoothingRate);
      this.mouseX += (this.mouseTX - this.mouseX) * k;
      this.mouseY += (this.mouseTY - this.mouseY) * k;
      // 鼠标左挪=视角右转(azim+)；鼠标上移=视角抬高(incl+)——只驱动星野
      parAz = -this.mouseX * CAMERA_PARALLAX.azimuthRange;
      parIncl = -this.mouseY * CAMERA_PARALLAX.inclinationRange;
    }
    return {
      incl: Math.min(CAMERA_INCL_CLAMP.max, Math.max(CAMERA_INCL_CLAMP.min, this.camCur.incl + driftIncl + parIncl)),
      zoom: this.camCur.zoom * driftZoom,
      azim: this.camCur.azim + driftAz + parAz,
      diskIncl: Math.min(CAMERA_INCL_CLAMP.max, Math.max(CAMERA_INCL_CLAMP.min, this.camCur.incl + driftIncl)),
      diskAzim: this.camCur.azim + driftAz,
      cx: this.baseCx + this.camCur.ox * this.cssW,
      cy: this.baseCy + this.camCur.oy * this.cssH,
    };
  }

  /* ================================================================
   * presence 目标值（含 v1 PAD 偏移）
   * ================================================================ */

  private currentTargets(): {speed: number; bright: number; turb: number; pulse: number} {
    const base = PRESENCE_STATES[this.stateName];
    // TODO(v1 简化映射，数值待实拍校准 —— 规范 §4.1 / 附录 B-2)
    const speed = Math.max(0, base.speed + this.padBias.a * 0.4);
    const bright = base.bright + this.padBias.p * 0.15;
    return {speed, bright, turb: base.turb, pulse: base.pulse};
  }

  private resolveHour(): number {
    return this.hour === null ? localClockHour() : this.hour;
  }

  /** 静态路径（fallback / reduced-motion）的重绘入口；动画路径由 rAF 每帧驱动 */
  private refreshStatic(): void {
    if (this.destroyed) return;
    if (this.fallback) this.drawFallback();
    else if (this.reduceMotion) this.renderOnceGL();
  }

  private updateClockTimer(): void {
    const need = !this.destroyed && this.hour === null && (this.fallback || this.reduceMotion);
    if (need && this.clockTimer === null) {
      this.clockTimer = window.setInterval(() => {
        if (!document.hidden) this.refreshStatic();
      }, CLOCK_REDRAW_MS);
    } else if (!need && this.clockTimer !== null) {
      clearInterval(this.clockTimer);
      this.clockTimer = null;
    }
  }

  private updateHoleCircleFromPreset(): void {
    const C: CameraPreset = CAMERA_PRESETS[this.camIdx];
    this.lastHole = {
      x: this.baseCx + C.ox * this.cssW,
      y: this.baseCy + C.oy * this.cssH,
      r: this.holeR * C.zoom,
    };
  }

  /* ================================================================
   * 事件处理（箭头函数字段，保证 removeEventListener 引用一致）
   * ================================================================ */

  private mouseHandler = (e: MouseEvent): void => {
    if (this.destroyed) return;
    if (this.interactive) {
      this.mouseTX = ((e.clientX - this.rectLeft) / this.cssW) * 2 - 1;
      this.mouseTY = ((e.clientY - this.rectTop) / this.cssH) * 2 - 1;
    }
    // 悬停指针提示（点击黑洞 = 切临渊机位）
    const c = this.lastHole;
    const dx = e.clientX - this.rectLeft - c.x;
    const dy = e.clientY - this.rectTop - c.y;
    const rr = c.r * 1.05;
    this.canvas.style.cursor = dx * dx + dy * dy <= rr * rr ? 'pointer' : '';
  };

  private resizeHandler = (): void => {
    if (this.destroyed) return;
    this.layout();
    if (this.fallback) {
      this.resizeFallback();
    } else {
      this.resizeGL();
      if (this.reduceMotion) this.renderOnceGL();
    }
    this.updateHoleCircleFromPreset();
  };

  private visibilityHandler = (): void => {
    if (document.hidden) this.stopLoop();
    else this.startLoop();
  };

  /* ================================================================
   * WebGL2 路径
   * ================================================================ */

  private compileShader(type: number, src: string): WebGLShader | null {
    const gl = this.gl;
    if (!gl) return null;
    const s = gl.createShader(type);
    if (!s) return null;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      console.warn('[scene] shader 编译失败:', gl.getShaderInfoLog(s));
      gl.deleteShader(s);
      return null;
    }
    return s;
  }

  private initGL(): boolean {
    try {
      this.gl = this.canvas.getContext('webgl2', {antialias: false, alpha: false, depth: false, stencil: false});
    } catch {
      this.gl = null;
    }
    const gl = this.gl;
    if (!gl) return false;
    const vs = this.compileShader(gl.VERTEX_SHADER, VERT);
    const fs = this.compileShader(gl.FRAGMENT_SHADER, FRAG);
    if (!vs || !fs) {
      this.gl = null;
      return false;
    }
    const prog = gl.createProgram();
    if (!prog) {
      this.gl = null;
      return false;
    }
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      console.warn('[scene] program 链接失败:', gl.getProgramInfoLog(prog));
      this.gl = null;
      return false;
    }
    gl.useProgram(prog);
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    for (const n of UNIFORM_NAMES) {
      this.uni[n] = gl.getUniformLocation(prog, n);
    }
    return true;
  }

  private resizeGL(): void {
    const gl = this.gl;
    if (!gl) return;
    this.canvas.width = Math.max(1, Math.round(this.cssW * this.dpr));
    this.canvas.height = Math.max(1, Math.round(this.cssH * this.dpr));
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
  }

  private drawGL(cam: EffectiveCamera): void {
    const gl = this.gl;
    if (!gl) return;
    const tp = timelineParams(this.resolveHour());
    gl.uniform2f(this.uni.uRes, this.canvas.width, this.canvas.height);
    gl.uniform2f(this.uni.uCenter, cam.cx * this.dpr, (this.cssH - cam.cy) * this.dpr);
    gl.uniform1f(this.uni.uScale, this.holeR * cam.zoom * this.dpr);
    gl.uniform1f(this.uni.uTime, this.tSec);
    gl.uniform1f(this.uni.uRot, this.rot);
    gl.uniform1f(this.uni.uBright, this.cur.bright);
    gl.uniform1f(this.uni.uTurb, this.cur.turb);
    gl.uniform1f(this.uni.uPulse, this.cur.pulse);
    gl.uniform1f(this.uni.uIncl, cam.incl);
    gl.uniform1f(this.uni.uAzim, cam.azim);
    gl.uniform1f(this.uni.uDiskAzim, cam.diskAzim);
    gl.uniform1f(this.uni.uDiskIncl, cam.diskIncl);
    gl.uniform1f(this.uni.uAmbStr, tp.amb);
    gl.uniform1f(this.uni.uEclipse, tp.eclipse);
    gl.uniform1f(this.uni.uTlBright, tp.bright);
    gl.uniform1f(this.uni.uTlGold, tp.gold);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    this.lastHole = {x: cam.cx, y: cam.cy, r: this.holeR * cam.zoom};
  }

  private loop = (now: number): void => {
    this.rafId = requestAnimationFrame(this.loop);
    const dt = Math.min((now - this.lastT) / 1000, PERFORMANCE.dtClampSec);
    this.lastT = now;
    this.tSec += dt;
    const tgt = this.currentTargets();
    const k = Math.min(1, dt * MOTION.stateSmoothingRate);
    this.cur.speed += (tgt.speed - this.cur.speed) * k;
    this.cur.bright += (tgt.bright - this.cur.bright) * k;
    this.cur.turb += (tgt.turb - this.cur.turb) * k;
    this.cur.pulse += (tgt.pulse - this.cur.pulse) * k;
    this.rot += this.cur.speed * dt;
    // 开发工具：?t0=<秒> 冻结时钟，供 headless 帧差验证；title 暴露内部状态供 --dump-dom 断言
    if (this.debugT0 !== null) {
      this.tSec = this.debugT0;
      this.rot = 0;
      document.title = `bh t=${this.tSec.toFixed(2)} bright=${this.cur.bright.toFixed(2)} turb=${this.cur.turb.toFixed(2)}`;
    }
    this.drawGL(this.effectiveCamera(dt, this.debugT0 !== null ? 0 : this.tSec));
  };

  private startLoop(): void {
    if (this.destroyed || this.fallback || this.reduceMotion || this.rafId !== null) return;
    this.lastT = performance.now();
    this.rafId = requestAnimationFrame(this.loop);
  }

  private stopLoop(): void {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
  }

  /** reduced-motion 静态单帧（原型 renderOnceGL：定格 rot=0.35 / t=1.2） */
  private renderOnceGL(): void {
    const tgt = this.currentTargets();
    this.cur = {speed: tgt.speed, bright: tgt.bright, turb: tgt.turb, pulse: tgt.pulse};
    this.rot = 0.35;
    this.tSec = 1.2;
    this.drawGL(this.effectiveCamera(0, 0));
  }

  /* ================================================================
   * Canvas 2D 降级 —— 静态、机位感知（倾角/视距/构图偏移），数值与原型一致
   * ================================================================ */

  private makeStars(): void {
    this.stars = [];
    const n = Math.round((this.cssW * this.cssH) / 6500);
    for (const L of STAR_LAYERS) {
      const m = Math.round(n * L.share);
      for (let i = 0; i < m; i++) {
        const hc = Math.random();
        this.stars.push({
          x: Math.random() * this.cssW,
          y: Math.random() * this.cssH,
          r: (0.6 + Math.random() * 0.9) * L.sizeMul,
          a: 0.1 + Math.random() * 0.5,
          k: L.k,
          c: hc < 0.12 ? '#aebfff' : hc < 0.55 ? '#f4ead2' : '#e8d3a0',
        });
      }
    }
  }

  private drawFallback(): void {
    const g = this.ctx2d;
    if (!g) return;
    const w = this.canvas.width;
    const h = this.canvas.height;
    const P = this.currentTargets();
    const C = CAMERA_PRESETS[this.camIdx];
    const cssW = this.cssW;
    const cssH = this.cssH;
    const dpr = this.dpr;
    const X = (this.baseCx + C.ox * cssW) * dpr;
    const Y = (this.baseCy + C.oy * cssH) * dpr;
    const R = this.holeR * C.zoom * dpr;
    this.lastHole = {x: this.baseCx + C.ox * cssW, y: this.baseCy + C.oy * cssH, r: this.holeR * C.zoom};
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.globalAlpha = 1;
    g.fillStyle = '#07070c';
    g.fillRect(0, 0, w, h);

    // starfield: three depth layers, parallax follows camera view angles
    const vox = C.azim;
    const voy = (C.incl - 0.3) * 0.8;
    for (const s of this.stars) {
      let px = s.x - vox * s.k * cssH;
      let py = s.y + voy * s.k * cssH;
      px = ((px % cssW) + cssW) % cssW;
      py = ((py % cssH) + cssH) % cssH;
      g.globalAlpha = s.a * 0.6;
      g.fillStyle = s.c;
      g.fillRect(px * dpr, py * dpr, Math.max(1, s.r * dpr * 0.8), Math.max(1, s.r * dpr * 0.8));
    }
    g.globalAlpha = 1;

    // faint milky-way band (far layer parallax)
    const nx = -0.485;
    const ny = 0.874; // band normal (GL y-up)
    const bc = 0.6 - voy * 0.45;
    const bx = (nx * bc - vox * 0.45) * cssH;
    const by = cssH - ny * bc * cssH;
    const grad = g.createLinearGradient(
      (bx - nx * cssH * 0.3) * dpr,
      (by + ny * cssH * 0.3) * dpr,
      (bx + nx * cssH * 0.3) * dpr,
      (by - ny * cssH * 0.3) * dpr,
    );
    grad.addColorStop(0, 'rgba(233,204,148,0)');
    grad.addColorStop(0.5, 'rgba(233,204,148,0.05)');
    grad.addColorStop(1, 'rgba(233,204,148,0)');
    g.fillStyle = grad;
    g.fillRect(0, 0, w, h);

    const amb = g.createRadialGradient(X, Y, R * 0.8, X, Y, R * 6.5);
    amb.addColorStop(0, 'rgba(255,210,122,' + (0.06 + 0.08 * P.bright).toFixed(3) + ')');
    amb.addColorStop(0.45, 'rgba(232,163,61,' + (0.03 + 0.04 * P.bright).toFixed(3) + ')');
    amb.addColorStop(1, 'rgba(232,163,61,0)');
    g.fillStyle = amb;
    g.fillRect(0, 0, w, h);

    const ringBand = (topClip: boolean, lift: number, alphaMul: number): void => {
      g.save();
      g.beginPath();
      if (topClip) g.rect(0, 0, w, Y);
      else g.rect(0, Y, w, h - Y);
      g.clip();
      g.translate(X, Y + lift);
      g.scale(1, C.incl);
      for (let j = 0; j < 16; j++) {
        const rr = R * (1.5 + j * 0.22);
        const a = 1 - j / 16;
        const gain = 0.35 + 0.65 * P.bright;
        g.beginPath();
        g.strokeStyle = 'rgba(255,236,200,' + (a * a * 0.36 * alphaMul * gain).toFixed(3) + ')';
        g.lineWidth = R * 0.15;
        g.arc(0, 0, rr, Math.PI * 0.5, Math.PI * 1.5);
        g.stroke();
        g.beginPath();
        g.strokeStyle = 'rgba(255,190,90,' + (a * a * 0.2 * alphaMul * gain).toFixed(3) + ')';
        g.lineWidth = R * 0.15;
        g.arc(0, 0, rr, -Math.PI * 0.5, Math.PI * 0.5);
        g.stroke();
      }
      g.restore();
    };
    ringBand(true, -R * 0.55 * (1 - C.incl), 0.8); // lensed arc over the top
    ringBand(false, 0, 1.0); // near side in front

    // photon ring glow (drawn BEFORE the horizon so the disk stays pure black)
    const ring = g.createRadialGradient(X, Y, R * 0.99, X, Y, R * 1.25);
    ring.addColorStop(0, 'rgba(255,243,214,' + (0.35 + 0.3 * P.bright).toFixed(3) + ')');
    ring.addColorStop(1, 'rgba(255,243,214,0)');
    g.fillStyle = ring;
    g.fillRect(X - R * 1.35, Y - R * 1.35, R * 2.7, R * 2.7);

    // pure black event horizon
    g.beginPath();
    g.fillStyle = '#000000';
    g.arc(X, Y, R, 0, Math.PI * 2);
    g.fill();

    // crisp photon rim
    g.beginPath();
    g.strokeStyle = 'rgba(255,243,214,' + (0.45 + 0.25 * P.bright).toFixed(3) + ')';
    g.lineWidth = Math.max(1.2, R * 0.012);
    g.arc(X, Y, R * 1.05, 0, Math.PI * 2);
    g.stroke();

    this.emitFallbackVisuals();
  }

  /** 降级路径 DOM 视觉：说话呼吸光位置 + 行照/凌日叠加层透明度（数值与原型 tlUI/updatePulseEl 一致） */
  private emitFallbackVisuals(): void {
    const cb = this.opts.onFallbackVisuals;
    if (!cb) return;
    const tp = timelineParams(this.resolveHour());
    const C = CAMERA_PRESETS[this.camIdx];
    const pulse: FallbackPulse | null =
      this.stateName === 'speaking'
        ? {
            x: this.baseCx + C.ox * this.cssW,
            y: this.baseCy + C.oy * this.cssH,
            diameter: this.holeR * C.zoom * 2.6,
          }
        : null;
    cb({
      pulse,
      ambOpacity: round3(tp.amb * 0.85 * (1 - tp.eclipse)),
      eclipseOpacity: round3(tp.eclipse * 0.75),
    });
  }

  private resizeFallback(): void {
    this.canvas.width = Math.max(1, Math.round(this.cssW * this.dpr));
    this.canvas.height = Math.max(1, Math.round(this.cssH * this.dpr));
    this.makeStars();
    this.drawFallback();
  }
}
