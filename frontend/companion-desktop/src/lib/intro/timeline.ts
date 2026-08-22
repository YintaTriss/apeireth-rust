/**
 * intro/timeline.ts — 开场动画「火之文明史」分拍时间轴（纯函数模块，无 DOM/框架依赖）
 *
 * 分镜来源: _research_mem/intro-storyboard/storyboard.md v1（十拍，总约 14.1s）。
 * 拍间 crossfade 0.3s（规范允许 0.2–0.4s），以拍界为中心对称窗：
 *   拍 i 在 [start_i - XF/2, start_i + XF/2] 内从 0 淡入到 1，
 *   拍 i-1 同窗从 1 淡出到 0 —— 任意时刻至多两拍同时活跃。
 * 入场拍 localT 含 XF/2 lead-in，保证淡入完成时火焰已「在烧」（循环粒子无冷启动）。
 *
 * 落幅接缝: INTRO_LANDING_START 起最后 1.5s 为落幅拍——IntroLayer 整体淡出，
 * 底下 SceneLayer/PlanetLayer/BridgeLayer 显形（三者在播放期间全程挂载，从不卸载）。
 */

import {sstep} from '../scene/timeline';

export type IntroBeatId =
  | 'spark' // 1 打火石：纯黑中迸出金色火花
  | 'campfire' // 2 营火：黑夜营火，火星升腾
  | 'furnace' // 3 蒸汽机：铆接炉门内火焰翻滚
  | 'engine' // 4 内燃机：气缸内燃烧火球脉动
  | 'primer' // 5 底火出膛：放射环 + 子弹旋出（此拍起不再剪）
  | 'flight' // 6 子弹飞行：火焰尾迹 + 激波锥
  | 'morph' // 7 变形：子弹熔成金色流线重塑为火箭
  | 'moon' // 8 划月：火箭划月，尾焰一道金线
  | 'arrival' // 9 抵达：金线弧线划向行星/星环方向
  | 'landing'; // 10 落幅：无缝交接活舰桥

export interface IntroBeatSpec {
  id: IntroBeatId;
  /** 本拍时长（秒） */
  dur: number;
}

/** 十拍时长（storyboard.md 验收节奏：前 5 拍快、后 5 拍渐慢，抵达段最舒缓） */
export const INTRO_BEATS: readonly IntroBeatSpec[] = [
  {id: 'spark', dur: 0.8},
  {id: 'campfire', dur: 1.2},
  {id: 'furnace', dur: 1.2},
  {id: 'engine', dur: 1.0},
  {id: 'primer', dur: 1.2},
  {id: 'flight', dur: 1.5},
  {id: 'morph', dur: 1.2},
  {id: 'moon', dur: 2.0},
  {id: 'arrival', dur: 2.5},
  {id: 'landing', dur: 1.5},
] as const;

/** 拍间 crossfade 半宽双侧 0.3s（storyboard 允许 0.2–0.4s） */
export const INTRO_XF = 0.3;

/** 各拍起点（秒），由时长累计 */
export const INTRO_STARTS: readonly number[] = (() => {
  const out: number[] = [];
  let acc = 0;
  for (const b of INTRO_BEATS) {
    out.push(acc);
    acc += b.dur;
  }
  return out;
})();

/** 总时长 ≈ 14.1s */
export const INTRO_TOTAL: number = INTRO_STARTS[INTRO_STARTS.length - 1] + INTRO_BEATS[INTRO_BEATS.length - 1].dur;

/** 落幅拍起点（跳过/快进的目标时刻） */
export const INTRO_LANDING_START: number = INTRO_STARTS[INTRO_STARTS.length - 1];

/** 一拍在时刻 t 的活跃权重与本拍本地时间 */
export interface BeatWeight {
  index: number;
  id: IntroBeatId;
  /** 权重 0..1（crossfade 窗内平滑过渡） */
  weight: number;
  /** 本拍本地时间（秒，含 XF/2 lead-in；循环粒子借此避免冷启动） */
  localT: number;
  /** 本拍进度 0..1 */
  tau: number;
}

/**
 * 时刻 t 的活跃拍集合（至多两拍）。t<0 或 t>INTRO_TOTAL 时仍返回端点拍。
 * 权重：拍 i 在 [start_i - XF/2, start_i + XF/2] 淡入，
 *       在 [start_{i+1} - XF/2, start_{i+1} + XF/2] 淡出；首拍无淡入、末拍无淡出。
 */
export function activeBeats(t: number): BeatWeight[] {
  const n = INTRO_BEATS.length;
  const out: BeatWeight[] = [];
  for (let i = 0; i < n; i++) {
    const start = INTRO_STARTS[i];
    const dur = INTRO_BEATS[i].dur;
    const end = start + dur;
    // 活跃区间向两侧各扩 XF/2（crossfade 窗）
    if (t < start - INTRO_XF / 2 || t > end + INTRO_XF / 2) continue;
    let w = 1;
    if (i > 0) w *= sstep(start - INTRO_XF / 2, start + INTRO_XF / 2, t);
    if (i < n - 1) w *= 1 - sstep(end - INTRO_XF / 2, end + INTRO_XF / 2, t);
    if (w <= 0.0005) continue;
    out.push({
      index: i,
      id: INTRO_BEATS[i].id,
      weight: w,
      localT: t - start + INTRO_XF / 2,
      tau: Math.min(1, Math.max(0, (t - start + INTRO_XF / 2) / dur)),
    });
  }
  return out;
}
