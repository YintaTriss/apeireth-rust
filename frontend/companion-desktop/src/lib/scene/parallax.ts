/**
 * parallax.ts — 视差开发覆写（?px=1&py=-1）
 *
 * BridgeLayer（近景舱室）与 PlanetLayer（中景环平原）共用：两轴都给出且为有限数时，
 * 强制视差目标到指定归一化位置（[-1, 1]，等价鼠标归一化坐标 mx/my），不再跟随鼠标；
 * 无参数或参数非法时返回 null，组件走正常 mousemove 追踪。配合 ?hour= 截图验收用。
 */

export interface ParallaxOverride {
  x: number;
  y: number;
}

export function readParallaxOverride(): ParallaxOverride | null {
  if (typeof window === 'undefined') return null;
  const q = new URLSearchParams(window.location.search);
  const px = q.get('px');
  const py = q.get('py');
  if (px === null || py === null || px.trim() === '' || py.trim() === '') return null;
  const x = Number(px);
  const y = Number(py);
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
  const clamp = (v: number): number => Math.min(1, Math.max(-1, v));
  return {x: clamp(x), y: clamp(y)};
}
