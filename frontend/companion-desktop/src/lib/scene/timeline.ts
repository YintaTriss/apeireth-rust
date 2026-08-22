/**
 * timeline.ts — 舰内时间线照明（纯函数模块，无 DOM/框架依赖）
 *
 * 移植自 frontend/design-preview/index.html v0.2 时间线滑杆逻辑（sstep/tlParams/tlPhaseName/tlTimeText），
 * 曲线与升降沿为 ✅ 已验收资产（规范 §3.2、令牌 timeline.curve）：
 *   - 黎明上升沿 04:30–08:00（3.5h，smoothstep）
 *   - 黄昏下降沿 14:00–18:00（4h，smoothstep，与黎明对称）
 *   - 凌日演示窗 12:00±20min（ec = 1 - sstep(0.20, 0.3334, |h-12)|）
 *   - 调制量 bright = 1 + 0.22*amb + 0.15*ec；gold = 1 - 0.55*amb
 */

/** 时间线环境参数 —— 直接对应 WebGL 路径的 uAmbStr/uEclipse/uTlBright/uTlGold 四个 uniform */
export interface TimelineParams {
  /** 行照环境蓝光强度 0..1 */
  amb: number;
  /** 凌日日食 0..1 */
  eclipse: number;
  /** 时间线亮度增益（乘性） */
  bright: number;
  /** 时间线金色占比 0..1 */
  gold: number;
}

export type TimelinePhaseId = 'daylit' | 'dusk' | 'night' | 'transit';

export interface TimelinePhase {
  id: TimelinePhaseId;
  label: string;
}

/** 黎明上升沿（h） */
export const TIMELINE_DAWN = {startH: 4.5, endH: 8.0} as const;
/** 黄昏下降沿（h） */
export const TIMELINE_DUSK = {startH: 14.0, endH: 18.0} as const;
/** 凌日演示窗：中心 12:00，±20min 开始软化（0.20h），±0.3334h 处完全退出 */
export const TIMELINE_ECLIPSE = {centerH: 12.0, softStartH: 0.2, softEndH: 0.3334} as const;
/** 调制系数 */
export const TIMELINE_MODULATION = {
  brightAmbGain: 0.22,
  brightEclipseGain: 0.15,
  goldAmbLoss: 0.55,
} as const;

/** 任意时刻折回 [0, 24) */
export function normalizeHour(hour: number): number {
  return ((hour % 24) + 24) % 24;
}

/** smoothstep（原型 sstep） */
export function sstep(a: number, b: number, x: number): number {
  const t = Math.max(0, Math.min(1, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
}

/**
 * 四档平滑曲线 → 环境参数。
 * amb:     sstep(4.5, 8.0, h) * (1 - sstep(14, 18, h))
 * eclipse: 1 - sstep(0.20, 0.3334, |h - 12|)
 */
export function timelineParams(hour: number): TimelineParams {
  const h = normalizeHour(hour);
  const amb =
    sstep(TIMELINE_DAWN.startH, TIMELINE_DAWN.endH, h) *
    (1 - sstep(TIMELINE_DUSK.startH, TIMELINE_DUSK.endH, h));
  const eclipse =
    1 - sstep(TIMELINE_ECLIPSE.softStartH, TIMELINE_ECLIPSE.softEndH, Math.abs(h - TIMELINE_ECLIPSE.centerH));
  return {
    amb,
    eclipse,
    bright: 1 + TIMELINE_MODULATION.brightAmbGain * amb + TIMELINE_MODULATION.brightEclipseGain * eclipse,
    gold: 1 - TIMELINE_MODULATION.goldAmbLoss * amb,
  };
}

/**
 * 档位名（原型 tlPhaseName，判定顺序一致：凌日优先）。
 * 注意「行照」标签从 06:00 起显示（原型行为），与 amb 上升沿 04:30 不同步——保持一致。
 */
export function timelinePhase(hour: number): TimelinePhase {
  const h = normalizeHour(hour);
  const ec =
    1 - sstep(TIMELINE_ECLIPSE.softStartH, TIMELINE_ECLIPSE.softEndH, Math.abs(h - TIMELINE_ECLIPSE.centerH));
  if (ec > 0.5) return {id: 'transit', label: '凌日 · 仪式时刻'};
  if (h >= 6 && h < 14) return {id: 'daylit', label: '行照'};
  if (h >= 14 && h < 18) return {id: 'dusk', label: '暮色'};
  return {id: 'night', label: '夜航'};
}

/** HH:MM（原型 tlTimeText） */
export function timelineTimeText(hour: number): string {
  const h = normalizeHour(hour);
  const hh = Math.floor(h);
  const mm = Math.floor((h - hh) * 60);
  const hs = hh < 10 ? '0' + hh : String(hh);
  const ms = mm < 10 ? '0' + mm : String(mm);
  return hs + ':' + ms;
}

/** 本地时钟 → 舰内时刻（小时，含分/秒小数，供 hour prop 为 null 时使用） */
export function localClockHour(now: Date = new Date()): number {
  return now.getHours() + now.getMinutes() / 60 + now.getSeconds() / 3600;
}
