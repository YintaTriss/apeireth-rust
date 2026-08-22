/**
 * tokens.ts — Apeireth 设计令牌的类型化 TS 镜像
 *
 * 数据源: frontend/design-preview/design-tokens.json v1.0.0
 * 规范:   docs/design/01-DESIGN-SYSTEM.md（§2 色彩 / §3 时间线 / §4 空间 / §6 排版 / §7 动效）
 *
 * 纪律: 标 ✅(accepted) 的数值与 frontend/design-preview/index.html v0.2 原型逐行一致；
 *       修改这些数值 = 改原型 + 改令牌 JSON，必须同 PR 提交（规范 §10 消费约定 1）。
 *       🟡 proposal / 🔵 calibration 分组在注释中逐组标注。
 */

/* ================================================================
 * 色板
 * ================================================================ */

export type RgbTriple = readonly [number, number, number];

/** ✅ 存在色梯度（金色 = 他的存在/活动专属色，禁止装饰性使用） */
export const COLOR_PRESENCE = {
  whiteHot: '#fff2d1',
  whiteHotRgb: [1.0, 0.95, 0.82] as RgbTriple,
  gold: '#ffd27a',
  goldRgb: [1.0, 0.824, 0.478] as RgbTriple,
  amber: '#e8a33d',
  amberRgb: [0.91, 0.639, 0.239] as RgbTriple,
  dopplerBoost: '#fff4d4',
  dopplerBoostRgb: [1.0, 0.955, 0.83] as RgbTriple,
  uiGold: '#e8d9b0',
  platinum: '#fff3d6',
} as const;

/** ✅ 深空底色（夜航/A 版基准）；事件视界纯黑 */
export const COLOR_VOID = {
  base: '#07070c',
  baseRgb: [0.027, 0.027, 0.047] as RgbTriple,
  horizon: '#000000',
} as const;

/** 🔵 行照/B 版深空蓝区间（概念图采样，待实拍校准） */
export const COLOR_DEEPSPACE_BLUE = {
  base: '#0d1b2e',
  dark: '#05162c',
  light: '#293b51',
  planetRim: '#2b5380',
  planetRimHi: '#6faadf',
  floor: '#1d2026',
  consoleWhite: '#919eb0',
} as const;

/** ✅ 文字骨白 */
export const COLOR_TEXT = {
  bone: '#e8e0cc',
} as const;

/** ✅ 星野星色与银河带 */
export const COLOR_STARFIELD = {
  tintWarm: '#f2d99e',
  tintWarmRgb: [0.95, 0.85, 0.62] as RgbTriple,
  tintBone: '#faedcc',
  tintBoneRgb: [0.98, 0.93, 0.8] as RgbTriple,
  tintBlue: '#b8ccff',
  tintBlueRgb: [0.72, 0.8, 1.0] as RgbTriple,
  milkyWay: '#e6cc94',
  milkyWayRgb: [0.9, 0.8, 0.58] as RgbTriple,
  milkyWayIntensity: 0.16,
} as const;

/** 🟡 UI 面板/卡片（终末地配方本地化提案，待主人确认 —— 附录 B-3） */
export const COLOR_UI = {
  panel: '#0b0d12',
  panelAlpha: 0.82,
  panelSolid: '#0b0d12',
  card: '#ece7da',
  cardDark: '#15181f',
  line: '#e8e0cc',
  lineAlpha: 0.14,
  accentTab: '#ffd27a',
  stamp: '#ece7da',
} as const;

/** 🟡 语义色（仅警示/否定/成功等语义用途，禁止装饰） */
export const COLOR_SEMANTIC = {
  success: '#7fb894',
  warning: '#d9a24a',
  danger: '#c0584e',
  info: '#7d9cc0',
} as const;

/** 🟡 反白仪式色（仅授权/承诺/原则批准时刻，日常界面禁止大面积白底） */
export const COLOR_INVERSION = {
  bg: '#e8e6e0',
  ink: '#141412',
} as const;

/** 聚合导出，便于 `COLOR.presence.gold` 式访问 */
export const COLOR = {
  presence: COLOR_PRESENCE,
  void: COLOR_VOID,
  deepspaceBlue: COLOR_DEEPSPACE_BLUE,
  text: COLOR_TEXT,
  starfield: COLOR_STARFIELD,
  ui: COLOR_UI,
  semantic: COLOR_SEMANTIC,
  inversion: COLOR_INVERSION,
} as const;

/* ================================================================
 * 排版（✅ 字族/字距/行高均出自原型与现行前端令牌）
 * ================================================================ */

export const TYPOGRAPHY = {
  /** 他的声音（引用/日记/正式发言）——衬线 */
  voice: {
    fontFamily: '"Songti SC", "Noto Serif CJK SC", "Noto Serif SC", "STSong", "SimSun", serif',
    quoteFontSize: 'clamp(16px, 1.45vw, 22px)',
    quoteLineHeight: 2.1,
    quoteLetterSpacingEm: 0.13,
    quoteMaxWidthCh: 31,
    quoteMobileFontSize: 'clamp(15px, 4.1vw, 19px)',
    quoteMobileLineHeight: 1.95,
    captionFontSizePx: 11,
    captionLetterSpacingEm: 0.5,
    attributionFontSizePx: 11,
    attributionLetterSpacingEm: 0.35,
  },
  /** UI 文字——无衬线系统栈 */
  ui: {
    fontFamily: 'system-ui, -apple-system, "PingFang SC", "Microsoft YaHei UI", sans-serif',
    labelFontSizePx: 12,
    labelLetterSpacingEm: 0.3,
    brandFontSizePx: 11,
    brandLetterSpacingEm: 0.55,
    footnoteFontSizePx: 10,
    footnoteLetterSpacingEm: 0.25,
  },
  /** 数据/编号/状态行——等宽 */
  data: {
    fontFamily: 'ui-monospace, "SF Mono", "Cascadia Mono", Consolas, monospace',
    fontSizePx: 11,
    letterSpacingEm: 0.28,
  },
} as const;

/* ================================================================
 * 动效时长（✅ 除时间线过渡外均出自原型）
 * ================================================================ */

export const MOTION = {
  /** 机位过渡时长（秒），easeInOutCubic */
  cameraTweenSec: 2.6,
  /** 机位/状态界面过渡下限（秒）——规则：一切界面过渡 ≥2s */
  uiTransitionMinSec: 2.0,
  easing: 'easeInOutCubic',
  /** presence 状态参数指数趋近速率（/s） */
  stateSmoothingRate: 2.4,
  /** 鼠标视差平滑速率（/s） */
  mouseSmoothingRate: 2.5,
  /** 说话呼吸（pulse 元素）：2.8s 循环 */
  breatheSec: 2.8,
  breatheOpacity: [0.22, 0.65] as readonly [number, number],
  breatheScale: [1.0, 1.09] as readonly [number, number],
  /** shader 内 pulseWave 角频率（周期 ≈3.3s） */
  pulseWaveAngularFreq: 1.9,
  /** 胶片颗粒刷新间隔（ms） */
  grainRefreshMs: 130,
  /** 🟡 时间线照明档位过渡（秒）：≥60，默认 90 —— 附录 B-1 待验收 */
  timelineTransitionMinSec: 60,
  timelineTransitionDefaultSec: 90,
} as const;

/* ================================================================
 * 相机 / 布局 / 星野（✅ 全部与原型一致，规范 §4）
 * ================================================================ */

/** 机位五元组：incl 盘面倾角压扁系数（小=平视）/ zoom 视距缩放 / azim 轨道方位角 / ox,oy 构图偏移（屏宽/高比例） */
export interface CameraPreset {
  id: number;
  label: string;
  incl: number;
  zoom: number;
  azim: number;
  ox: number;
  oy: number;
}

export const CAMERA_PRESETS: readonly CameraPreset[] = [
  // 波次 3b：远眺 zoom 1.0 → 0.6，黑洞视大小缩到约 60%（只缩默认远眺机位；
  // 临渊/俯瞰机位与着色器光学不动，命中区域随 holeR*zoom 自动适配）。
  // 注：本组为 ✅ accepted 令牌，按规范 §10 纪律需与设计令牌 JSON/原型同 PR 对齐。
  {id: 0, label: '远眺', incl: 0.3, zoom: 0.6, azim: 0.0, ox: 0.0, oy: 0.0},
  {id: 1, label: '临渊', incl: 0.115, zoom: 1.9, azim: -0.1, ox: 0.0, oy: -0.05},
  {id: 2, label: '俯瞰', incl: 0.8, zoom: 0.92, azim: 0.4, ox: 0.0, oy: 0.03},
];

/** incl 活动范围 */
export const CAMERA_INCL_CLAMP = {min: 0.06, max: 1.2} as const;

/** 机位快捷键 */
export const CAMERA_HOTKEYS = ['1', '2', '3'] as const;

/** 自动漂移：三组正弦叠加，周期互质（47/37/53），漂移永不可被观众预测节奏 */
export const CAMERA_DRIFT = {
  azimuth: {amplitude: 0.05, periodSec: 47, phase: 0.0},
  inclination: {amplitude: 0.02, periodSec: 37, phase: 1.3},
  /** 乘性漂移：zoom *= 1 + amplitude * sin(...) */
  zoom: {amplitude: 0.045, periodSec: 53, phase: 2.1},
} as const;

/**
 * 轨道式鼠标视差：鼠标只改 azim/incl 两个轨道角；黑洞构图中心永远钉死（位移 0px）。
 * 波次 3f 起该通道只驱动星野三层视差（shader 内 voff）；盘面姿态改由
 * uDiskAzim/uDiskIncl 承担（不含鼠标项），盘面/黑洞对鼠标零响应。
 * 波次 3g：azim/incl 幅度降到约 1/3（0.18→0.055 / 0.07→0.022），修正深度倒挂——
 * 星野是无限远背景，鼠标全行程近层位移须 ≤2px，不得比行星/舰桥等前景更显眼。
 * 方向约定：鼠标左挪 = 视角右转(azim+)；鼠标上移 = 视角抬高(incl+)。
 */
export const CAMERA_PARALLAX = {
  azimuthRange: 0.055,
  inclinationRange: 0.022,
  smoothingRate: 2.5,
} as const;

/** 黑洞构图布局：桌面端右半屏偏上；移动端 ≤820px */
export const SCENE_LAYOUT = {
  breakpointPx: 820,
  desktop: {cxRatio: 0.66, cyRatio: 0.45, holeRadiusRatio: 0.16},
  mobile: {cxRatio: 0.5, cyRatio: 0.28, holeRadiusRatio: 0.19},
} as const;

/** 星野三层视差（近层位移 > 远层；视角右转 → 星野向左流） */
export interface StarLayerSpec {
  /** 视差系数 */
  k: number;
  /** 密度 scale */
  scale: number;
  /** 星点阈值 thr */
  threshold: number;
  /** 尺寸倍率 */
  sizeMul: number;
  /** 数量占比（2D 兜底用） */
  share: number;
}

export const STAR_LAYERS: readonly StarLayerSpec[] = [
  {k: 0.5, scale: 230.0, threshold: 0.996, sizeMul: 0.8, share: 0.5},
  {k: 0.9, scale: 150.0, threshold: 0.9955, sizeMul: 1.15, share: 0.33},
  {k: 1.4, scale: 95.0, threshold: 0.9965, sizeMul: 1.7, share: 0.17},
];

/** 银河带 */
export const MILKY_WAY = {
  normalX: -0.485,
  normalY: 0.874,
  bandOffset: 0.6,
  bandWidth: 2.2,
  parallaxK: 0.45,
  /** 带内星星增亮 starBoost = 1 + starBoostGain * mw */
  starBoostGain: 2.2,
} as const;

/* ================================================================
 * presence 三态（✅ 与原型 STATES 逐行一致，规范 §4.1）
 * ================================================================ */

export type SceneMode = 'quiet' | 'thinking' | 'speaking';

export interface PresenceStateParams {
  label: string;
  speed: number;
  bright: number;
  turb: number;
  pulse: number;
}

export const PRESENCE_STATES: Record<SceneMode, PresenceStateParams> = {
  quiet: {label: '安静', speed: 0.015, bright: 0.32, turb: 0.12, pulse: 0.0},
  thinking: {label: '思考', speed: 0.38, bright: 0.9, turb: 1.0, pulse: 0.0},
  speaking: {label: '说话', speed: 0.16, bright: 1.05, turb: 0.35, pulse: 1.0},
};

/** 状态参数指数趋近速率（/s），同 MOTION.stateSmoothingRate */
export const PRESENCE_SMOOTHING_RATE = 2.4;

/* ================================================================
 * 时间线四档照明参数（🟡 proposal —— 时段与升降沿已验收，五参数待滑杆验收，附录 B-1）
 * ================================================================ */

export interface TimelineModeSpec {
  label: string;
  ambientColorTempK: number;
  ambientIntensity: number;
  panelOpacity: number;
  starBrightness: number;
  goldRatio: number;
}

export const TIMELINE_MODES: Record<'daylit' | 'dusk' | 'night' | 'transit', TimelineModeSpec> = {
  daylit: {label: '行照', ambientColorTempK: 6200, ambientIntensity: 0.62, panelOpacity: 0.58, starBrightness: 1.25, goldRatio: 0.14},
  dusk: {label: '暮色', ambientColorTempK: 4300, ambientIntensity: 0.38, panelOpacity: 0.7, starBrightness: 1.0, goldRatio: 0.24},
  night: {label: '夜航', ambientColorTempK: 2900, ambientIntensity: 0.16, panelOpacity: 0.85, starBrightness: 0.85, goldRatio: 0.36},
  transit: {label: '凌日', ambientColorTempK: 2400, ambientIntensity: 0.06, panelOpacity: 0.92, starBrightness: 0.3, goldRatio: 0.6},
};

/* ================================================================
 * 性能纪律（✅ 出自原型）
 * ================================================================ */

export const PERFORMANCE = {
  dprMax: 2,
  dtClampSec: 0.05,
  pauseWhenHidden: true,
} as const;
