/**
 * fire.ts — 开场动画「火之文明史」GPU 程序化火焰引擎（框架无关 TS 模块，零依赖）
 *
 * 风格与性能纪律对齐 ../scene/blackhole.ts：
 *   原生 WebGL2、uniform 命名 u*、DPR ≤ PERFORMANCE.dprMax、dt clamp、
 *   document.hidden 停 rAF、destroy() 主动 loseContext；
 *   开发工具 ?it=<秒> 冻结开场时钟供无头截图（照抄 ?t0= 纪律），title 暴露内部状态。
 *
 * 渲染结构（每帧）：
 *   1. 背景 matte pass（全屏 quad，非混合）：深空底 / 星野(可水平拉成速度线) / 银河 /
 *      月球暗盘+边缘光 / 炉膛铁环+铆钉+蒸汽 / 发动机气缸剖面 / 激波锥弧 /
 *      弹体-火箭金属 capsule / 营火地面光 / 火心体积光 / 闪光 / 暗角 / 落幅行星暗示。
 *      每拍一套特征强度 uniform，拍间 crossfade 直接对参数集做数值 lerp。
 *   2. 火焰粒子 pass（GL_POINTS，additive ONE/ONE）：位置全部在顶点着色器内由
 *      aSeed 属性 + uniform 编舞解算（闭环阻力积分 + 正弦湍流场 + 弹道回溯原点），
 *      软 sprite 衰减 + 速度向拉伸（火花/彗尾 streak）。每拍一套 uniform 编舞，
 *      crossfade 期间对两拍各 draw 一次（uAlpha=拍权重）。
 *
 * 匹配剪辑纪律：拍 1–5 火心钉在屏幕中心附近（"同一簇火"）；拍 5 起 origin 随子弹
 * 加速移动（JS 闭式弹道 + 数值微分得 p/v/a 三 uniform，顶点着色器按 age 回溯发射点，
 * 形成弯曲拖尾）。拍 8 火箭跨屏划月，拍 9 金线 ease-out 弧线减速划向行星/星环方向，
 * 拍 10 粒子归零、背景收敛为产品场景近似（行星在左、星环沙白），配合 DOM 淡出完成接缝。
 */

import {COLOR_PRESENCE, COLOR_STARFIELD, COLOR_VOID, PERFORMANCE} from '../scene/tokens';
import {
  INTRO_LANDING_START,
  INTRO_TOTAL,
  activeBeats,
  type BeatWeight,
  type IntroBeatId,
} from './timeline';

/** 粒子总数有界（顶点着色器内按 uCount01 门控活跃比例） */
const MAX_PARTICLES = 4096;

type Vec2 = readonly [number, number];
type Vec3 = readonly [number, number, number];

/* ================================================================
 * 编舞参数表类型
 * ================================================================ */

/** 背景 matte 特征参数（全部可数值 lerp；强度类 0=关闭） */
interface BgParams {
  stars: number; // 星野强度
  starStretch: number; // 星点水平拉伸（速度线）
  milky: number; // 银河带（右侧）
  moon: number; // 月球暗盘
  moonPos: Vec2;
  moonR: number;
  furnace: number; // 炉膛铁环
  furnaceR: number;
  engine: number; // 发动机气缸剖面
  shock: number; // 激波锥弧
  shockApex: Vec2;
  body: number; // 弹体/火箭 capsule
  bodyPos: Vec2;
  bodyLen: number;
  bodyMolten: number; // 熔融金流占比
  bodyMorph: number; // 0 子弹 → 1 火箭
  planetHint: number; // 落幅行星/星环暗示（对齐 PlanetLayer 构图）
  groundGlow: number; // 营火地面光
  groundPos: Vec2;
  coreGlow: number; // 火心体积光
  corePos: Vec2;
  flash: number; // 闪光（打火石/点火/底火）
  flashPos: Vec2;
  vig: number; // 暗角
}

/** 火焰粒子 uniform 编舞（每拍一套；含义见顶点着色器） */
interface FireParams {
  origin: Vec2; // 静态拍的发射原点（动态拍由 originTrack 覆盖）
  dir: Vec2; // 主喷射方向
  spread: number; // 方向散布（弧度，±）
  spawnR: number; // 发射口半径抖动
  speed: number;
  speedVar: number;
  inherit: number; // 继承弹道速度比例（拖尾贴弹体/留痕世界）
  life: number;
  lifeVar: number;
  size0: number; // 出生尺寸（CSS px）
  size1: number; // 消亡尺寸
  sizeVar: number;
  grav: Vec2;
  drag: number;
  turb: number; // 湍流位移幅度（随 age 增长）
  turbFreq: number;
  turbTime: number;
  count01: number; // 活跃粒子比例
  burst: 0 | 1; // 1 = 一次性迸发（birth 散布在 burstSpan 内）
  burstSpan: number;
  radial01: number; // 放射环粒子占比（底火）
  radialSpeed: number;
  radialDrag: number;
  radialLife: number;
  soft01: number; // 大软光点占比（营火体积感）
  softSize: number;
  softGain: number;
  streak: number; // 速度向拉伸
  twinkle: number; // 闪烁幅度
  ember01: number; // 火星子类占比（更快/更长寿命/更细）
  gain: number; // 整体亮度增益
  colHot: Vec3;
  colMid: Vec3;
  colTail: Vec3;
}

/** 动态弹道（moving origin 拍）返回值 */
interface OriginTrack {
  p: Vec2;
  v: Vec2;
  a: Vec2;
}

/* ================================================================
 * GLSL —— 背景 matte pass
 * ================================================================ */

const BG_VERT = `#version 300 es
layout(location=0) in vec2 aPos;
void main(){ gl_Position = vec4(aPos, 0.0, 1.0); }`;

const BG_FRAG = `#version 300 es
precision highp float;
uniform vec2  uRes;
uniform float uAspect;
uniform float uTime;
uniform float uStars;
uniform float uStarStretch;
uniform float uMilky;
uniform float uMoon;
uniform vec2  uMoonPos;
uniform float uMoonR;
uniform float uFurnace;
uniform float uFurnaceR;
uniform float uEngine;
uniform float uShock;
uniform vec2  uShockApex;
uniform float uBody;
uniform vec2  uBodyPos;
uniform float uBodyLen;
uniform float uBodyMolten;
uniform float uBodyMorph;
uniform float uPlanetHint;
uniform float uGroundGlow;
uniform vec2  uGroundPos;
uniform float uCoreGlow;
uniform vec2  uCorePos;
uniform float uFlash;
uniform vec2  uFlashPos;
uniform float uVig;
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

void main(){
    // y-up 纵横比修正坐标：y ∈ [-1,1]，x ∈ [-aspect, aspect]
    vec2 suv = (gl_FragCoord.xy / uRes.y) * 2.0 - vec2(uAspect, 1.0);
    vec3 col = vec3(${COLOR_VOID.baseRgb.join(',')});

    // ---- 银河带（拍 8 右侧） ----
    if (uMilky > 0.001) {
        vec2 nrm = vec2(-0.485, 0.874);
        float bd = (dot(suv, nrm) - 0.55) * 2.0;
        float mw = exp(-bd * bd);
        float tex = fbm(suv * 2.6 + 4.7);
        float side = smoothstep(-0.4, 0.9, suv.x / uAspect);
        col += vec3(${COLOR_STARFIELD.milkyWayRgb.join(',')}) * mw * (0.25 + 0.6 * tex) * 0.13 * side * uMilky;
    }

    // ---- 星野（单层；uStarStretch 拉成水平速度线，拍 6/7） ----
    if (uStars > 0.001) {
        vec2 g = (suv + vec2(uAspect, 1.0)) * 95.0;
        vec2 id = floor(g);
        float h = hash21(id);
        if (h > 0.9955) {
            vec2 jit = vec2(hash21(id + 7.13), hash21(id + 3.71)) - 0.5;
            vec2 off = fract(g) - 0.5 - jit * 0.7;
            off.x /= 1.0 + uStarStretch * 7.0;
            float sd = length(off);
            float mag = (h - 0.9955) / 0.0045;
            float s = 1.0 - smoothstep(0.0, 0.16 * (0.6 + 0.8 * mag), sd);
            float hc = hash21(id + 17.7);
            vec3 tint = vec3(${COLOR_STARFIELD.tintWarmRgb.join(',')});
            if (hc < 0.12) tint = vec3(${COLOR_STARFIELD.tintBlueRgb.join(',')});
            else if (hc < 0.55) tint = vec3(${COLOR_STARFIELD.tintBoneRgb.join(',')});
            col += tint * s * (0.08 + 0.5 * mag) * uStars;
        }
    }

    // ---- 月球（拍 8：暗盘 + 右下边缘光，概念图 beat-08） ----
    if (uMoon > 0.001) {
        vec2 mp = suv - uMoonPos;
        float mr = length(mp);
        vec2 L = normalize(vec2(0.55, -0.35));
        float nl = dot(mp / max(mr, 1e-4), L);
        float disk = 1.0 - smoothstep(uMoonR - 0.004, uMoonR + 0.002, mr);
        float crat = fbm(mp * 9.0 / uMoonR + 3.1) * 0.6 + fbm(mp * 23.0 / uMoonR) * 0.4;
        float lit = smoothstep(-0.35, 0.85, nl);
        vec3 surf = mix(vec3(0.010, 0.010, 0.014), vec3(0.40, 0.42, 0.46) * (0.40 + 0.60 * crat), lit);
        col = mix(col, surf, disk * 0.96 * uMoon);
        float rim = exp(-pow((mr - uMoonR) * 60.0, 2.0)) * smoothstep(-0.15, 0.7, nl);
        col += vec3(0.85, 0.88, 0.92) * rim * 0.75 * uMoon;
        col += vec3(0.50, 0.55, 0.65) * exp(-max(mr - uMoonR, 0.0) * 9.0) * 0.045 * uMoon;
    }

    // ---- 落幅行星暗示（拍 9/10：星肢左上 + 沙白环平原，对齐 PlanetLayer 构图） ----
    if (uPlanetHint > 0.001) {
        float horizon = -0.24; // 视口 62%（PlanetLayer 3f 实测地平线锚点）
        float towardHole = smoothstep(-0.3 * uAspect, 0.9 * uAspect, suv.x);
        float below = 1.0 - smoothstep(horizon - 0.02, horizon + 0.015, suv.y);
        float ringTex = fbm(vec2(suv.x * 3.0, suv.y * 40.0));
        vec3 sand = vec3(0.62, 0.56, 0.44) * (0.30 + 0.70 * towardHole) * (0.70 + 0.55 * ringTex);
        col = mix(col, sand, below * uPlanetHint * 0.85);
        col += vec3(0.95, 0.85, 0.60) * exp(-pow((suv.y - horizon) * 30.0, 2.0)) * (0.10 + 0.28 * towardHole) * uPlanetHint;
        vec2 pp = suv - vec2(-0.60 * uAspect, 0.75);
        float pr = length(pp);
        float limb = (1.0 - smoothstep(1.16, 1.22, pr)) * smoothstep(horizon - 0.04, horizon + 0.10, suv.y);
        float limbShade = smoothstep(1.16, 0.55, pr);
        vec3 limbCol = mix(vec3(0.26, 0.25, 0.23), vec3(0.76, 0.73, 0.64), limbShade);
        limbCol *= 0.85 + 0.3 * fbm(pp * vec2(2.0, 6.0));
        col = mix(col, limbCol, limb * uPlanetHint * 0.9);
    }

    // ---- 营火地面光（拍 2） ----
    if (uGroundGlow > 0.001) {
        vec2 gq = (suv - uGroundPos) * vec2(1.0, 2.4);
        col += vec3(1.0, 0.50, 0.15) * uGroundGlow * 0.35 * exp(-dot(gq, gq) * 6.0);
    }

    // ---- 发动机气缸剖面（拍 4：中央燃烧腔 + 四周暗钢 + 内缘金线 + 火花塞） ----
    if (uEngine > 0.001) {
        float ax = abs(suv.x);
        float chamber = (1.0 - smoothstep(0.33, 0.37, ax))
                      * smoothstep(-0.42, -0.36, suv.y)
                      * (1.0 - smoothstep(0.36, 0.42, suv.y));
        float metal = 1.0 - chamber;
        float tex = fbm(suv * vec2(7.0, 14.0));
        vec3 steel = vec3(0.026, 0.028, 0.034) + vec3(0.10, 0.11, 0.12) * tex * 0.5;
        float rimIn = exp(-pow((ax - 0.345) * 40.0, 2.0)) * step(abs(suv.y), 0.40)
                    + exp(-pow((suv.y - 0.385) * 40.0, 2.0)) * step(ax, 0.345)
                    + exp(-pow((suv.y + 0.385) * 40.0, 2.0)) * step(ax, 0.345);
        steel += vec3(1.0, 0.60, 0.22) * rimIn * (1.0 - chamber) * 0.28;
        float plug = exp(-pow(length((suv - vec2(0.0, 0.47)) * vec2(14.0, 20.0)), 2.0));
        steel += vec3(0.60, 0.62, 0.66) * plug * 0.45;
        col = mix(col, steel, metal * 0.97 * uEngine);
    }

    // ---- 炉膛铁环（拍 3：铆接铁环 + 内缘火光照亮 + 外缘蒸汽） ----
    if (uFurnace > 0.001) {
        float fr = length(suv);
        float ring1 = uFurnaceR + 0.30;
        float iron = smoothstep(uFurnaceR - 0.008, uFurnaceR + 0.008, fr)
                   * (1.0 - smoothstep(ring1 - 0.02, ring1 + 0.06, fr));
        float ang = atan(suv.y, suv.x);
        float rivet = pow(0.5 + 0.5 * cos(ang * 14.0), 24.0)
                    * smoothstep(uFurnaceR + 0.10, uFurnaceR + 0.16, fr)
                    * (1.0 - smoothstep(uFurnaceR + 0.20, uFurnaceR + 0.26, fr));
        float tex = fbm(suv * 9.0);
        vec3 ironCol = vec3(0.028, 0.026, 0.028) + vec3(0.05, 0.045, 0.04) * tex;
        ironCol += vec3(0.90, 0.55, 0.20) * rivet * 0.10;
        col = mix(col, ironCol, iron * 0.98 * uFurnace);
        col += vec3(1.0, 0.62, 0.22) * exp(-pow((fr - uFurnaceR) * 26.0, 2.0)) * 0.50 * uFurnace;
        float steam = fbm(suv * 3.0 + vec2(uTime * 0.10, -uTime * 0.06));
        float steamMask = smoothstep(ring1 + 0.05, ring1 + 0.35, fr) * (1.0 - smoothstep(1.0, 1.45, fr));
        col += vec3(0.55, 0.58, 0.62) * steam * steamMask * 0.05 * uFurnace;
    }

    // ---- 激波锥弧（拍 6/7：弹头前方嵌套弧 + 马赫锥边线） ----
    if (uShock > 0.001) {
        vec2 sp = suv - uShockApex;
        float sr = length(sp);
        float sang = abs(atan(sp.y, sp.x));
        float sector = 1.0 - smoothstep(0.25, 0.75, sang);
        float arcs = pow(0.5 + 0.5 * sin(sr * 34.0 - uTime * 7.0), 6.0) * exp(-sr * 1.9);
        col += vec3(0.75, 0.82, 0.95) * arcs * sector * 0.10 * uShock;
        float cone = exp(-pow((sang - 0.42 - sr * 0.10) * 14.0, 2.0)) * exp(-sr * 1.4);
        col += vec3(0.80, 0.85, 1.0) * cone * 0.06 * uShock;
    }

    // ---- 弹体/火箭 capsule（拍 5–8：拉丝金属 + 熔融金流 + 尾端被火焰照亮） ----
    if (uBody > 0.001) {
        vec2 bp = suv - uBodyPos;
        float rad = mix(0.036, 0.028, uBodyMorph);
        float hl2 = uBodyLen * 0.5 - rad;
        float taper = mix(0.65, 0.92, uBodyMorph);
        float rr = rad * (1.0 - taper * smoothstep(hl2 - rad, hl2 + rad * 2.2, bp.x));
        float d = length(vec2(max(abs(bp.x) - hl2, 0.0), bp.y)) - rr;
        float inside = 1.0 - smoothstep(-0.004, 0.004, d);
        if (inside > 0.0) {
            float brush = fbm(vec2(bp.x * 40.0, bp.y * 90.0));
            vec3 metalC = mix(vec3(0.16, 0.12, 0.07), vec3(0.55, 0.42, 0.22),
                              smoothstep(-rad, rad, bp.y) * 0.6 + brush * 0.25);
            float moltenBand = fbm(vec2(bp.x * 10.0 - uTime * 2.0, bp.y * 6.0));
            vec3 melt = vec3(1.0, 0.72, 0.30) * (0.6 + 0.8 * moltenBand);
            metalC = mix(metalC, melt, uBodyMolten * (0.35 + 0.65 * smoothstep(0.2, 0.9, moltenBand)));
            metalC += vec3(1.0, 0.60, 0.20) * smoothstep(-hl2 * 0.4, -hl2, bp.x) * 0.5;
            col = mix(col, metalC, inside * 0.95 * uBody);
        }
    }

    // ---- 火心体积光 / 闪光 / 暗角 ----
    if (uCoreGlow > 0.001) {
        vec2 cq = suv - uCorePos;
        col += vec3(1.0, 0.72, 0.30) * uCoreGlow * 0.55 * exp(-dot(cq, cq) * 4.5);
    }
    if (uFlash > 0.001) {
        vec2 fq = suv - uFlashPos;
        col += vec3(1.0, 0.96, 0.85) * uFlash * exp(-dot(fq, fq) * 3.5);
    }
    col *= 1.0 - uVig * smoothstep(0.55, 1.35, length(suv * vec2(0.72, 1.0)));

    fragColor = vec4(col, 1.0);
}`;

/* ================================================================
 * GLSL —— 火焰粒子 pass（GL_POINTS，additive）
 * ================================================================ */

const FIRE_VERT = `#version 300 es
precision highp float;
layout(location=0) in vec4 aSeed;
uniform float uAspect;
uniform float uDpr;
uniform float uTime;
uniform vec2  uP0;
uniform vec2  uVel;
uniform vec2  uAcc;
uniform vec2  uDir;
uniform float uSpread;
uniform float uSpawnR;
uniform float uSpeed;
uniform float uSpeedVar;
uniform float uInherit;
uniform float uLife;
uniform float uLifeVar;
uniform float uSize0;
uniform float uSize1;
uniform float uSizeVar;
uniform vec2  uGrav;
uniform float uDrag;
uniform float uTurb;
uniform float uTurbFreq;
uniform float uTurbTime;
uniform float uCount01;
uniform float uBurst;
uniform float uBurstSpan;
uniform float uRadial01;
uniform float uRadialSpeed;
uniform float uRadialDrag;
uniform float uRadialLife;
uniform float uSoft01;
uniform float uSoftSize;
uniform float uSoftGain;
uniform float uStreak;
uniform float uTwinkle;
uniform float uGain;
uniform float uEmber01;
uniform float uAlpha;
uniform vec3  uColHot;
uniform vec3  uColMid;
uniform vec3  uColTail;
out vec3 vCol;
out float vAlpha;
out float vStreak;
out vec2 vAxis;
out float vSoft;

void main(){
    float r1 = aSeed.x;
    float r2 = aSeed.y;
    float r3 = aSeed.z;
    float r4 = aSeed.w;
    float gate = fract(r1 * 7.31 + r2 * 3.17 + r4 * 1.93);
    bool live = gate < uCount01; // 「active」是 GLSL 保留字，禁用
    bool ember = r4 > 1.0 - uEmber01;
    bool radial = r3 < uRadial01;
    bool soft = (!ember) && (!radial) && (fract(r3 * 5.7) < uSoft01);

    float life = (radial ? uRadialLife : uLife)
               * (1.0 + (r1 - 0.5) * 2.0 * uLifeVar)
               * (ember ? 1.7 : 1.0);
    life = max(life, 0.05);
    float birth = uBurst > 0.5 ? r2 * (radial ? min(uBurstSpan, 0.18) : uBurstSpan) : 0.0;
    float ageSec = uBurst > 0.5 ? (uTime - birth) : fract(uTime / life + r2) * life;

    if (!live || ageSec < 0.0 || ageSec > life) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        gl_PointSize = 0.0;
        vCol = vec3(0.0);
        vAlpha = 0.0;
        vStreak = 0.0;
        vAxis = vec2(1.0, 0.0);
        vSoft = 0.0;
        return;
    }
    float age01 = ageSec / life;

    // 发射原点：沿弹道的过去时刻位置（moving origin 拍的拖尾核心）
    vec2 org = uP0 - uVel * ageSec + 0.5 * uAcc * ageSec * ageSec;
    vec2 orgVel = uVel - uAcc * ageSec;
    float ja = r2 * 6.28318 + r4 * 2.1;
    org += vec2(cos(ja), sin(ja)) * uSpawnR * (0.3 + 0.7 * r1);

    float ang;
    float spd;
    float drag;
    if (radial) {
        ang = r2 * 6.28318 + r1 * 0.7;
        spd = uRadialSpeed * (0.75 + 0.5 * r4);
        drag = uRadialDrag;
    } else {
        ang = atan(uDir.y, uDir.x) + (r3 - 0.5) * 2.0 * uSpread + (ember ? (r2 - 0.5) * 0.9 : 0.0);
        spd = uSpeed * (1.0 + (r1 - 0.5) * 2.0 * uSpeedVar) * (ember ? 1.9 : 1.0);
        drag = uDrag;
    }
    vec2 v0 = vec2(cos(ang), sin(ang)) * spd + orgVel * uInherit;

    // 阻力闭式积分 + 重力
    float e = exp(-drag * ageSec);
    vec2 disp;
    if (drag > 0.001) {
        float k = (1.0 - e) / drag;
        disp = v0 * k + uGrav * (ageSec - k) / drag;
    } else {
        disp = v0 * ageSec + 0.5 * uGrav * ageSec * ageSec;
    }

    // 湍流（随年龄增长的正弦场扰动）
    vec2 tp = org * uTurbFreq + vec2(r4 * 17.0, r1 * 13.0);
    float tn = uTime * uTurbTime;
    vec2 turb = vec2(
        sin(tp.y * 1.7 + tn * 1.9 + sin(tp.x * 1.3 + tn) * 1.2),
        cos(tp.x * 1.9 - tn * 1.7 + sin(tp.y * 1.1 - tn * 1.3) * 1.2)
    ) * (uTurb * age01 * (0.4 + 0.6 * r2));

    vec2 pos = org + disp + turb;
    gl_Position = vec4(pos.x / uAspect, pos.y, 0.0, 1.0);

    float size = mix(uSize0, uSize1, age01) * (1.0 + (r4 - 0.5) * 2.0 * uSizeVar) * (ember ? 0.45 : 1.0);
    if (soft) size = uSoftSize * (0.65 + 0.7 * r1);
    gl_PointSize = size * uDpr;

    float env = smoothstep(0.0, 0.07, age01) * pow(1.0 - age01, 1.35);
    float tw = 1.0 - uTwinkle * (0.5 + 0.5 * sin(uTime * (18.0 + 26.0 * r3) + r4 * 40.0));
    vAlpha = env * tw * uAlpha * uGain * (soft ? uSoftGain : 1.0);

    vec3 c = mix(uColHot, uColMid, smoothstep(0.0, 0.30, age01));
    c = mix(c, uColTail, smoothstep(0.30, 1.0, age01));
    vCol = c;

    // 当前速度（streak 拉伸轴；高 inherit 拖尾粒子退回取弹道方向）
    vec2 vcur = v0 * e + uGrav * ageSec + orgVel * (1.0 - uInherit);
    if (dot(vcur, vcur) < 0.0025) vcur = uVel;
    vAxis = normalize(vcur + vec2(1e-5, 0.0));
    vStreak = uStreak * (ember ? 1.3 : 1.0) * (radial ? 1.2 : 1.0);
    vSoft = soft ? 1.0 : 0.0;
}`;

const FIRE_FRAG = `#version 300 es
precision highp float;
in vec3 vCol;
in float vAlpha;
in float vStreak;
in vec2 vAxis;
in float vSoft;
out vec4 fragColor;
void main(){
    vec2 d = gl_PointCoord * 2.0 - 1.0;
    vec2 t = vAxis;
    vec2 n = vec2(-t.y, t.x);
    vec2 p = vec2(dot(d, t), dot(d, n)); // 速度向/法向坐标（未压缩）
    vec2 q = vec2(p.x / (1.0 + vStreak * 7.0), p.y); // 沿速度向拉伸（火花/彗尾 streak）
    float r2 = dot(q, q);
    // streak 粒子横向收窄：长宽比随 vStreak 拉大——金丝而非金点（拍 1 验收修复）
    float thin = 1.0 + vStreak * 3.0;
    float m = vSoft > 0.5
        ? exp(-r2 * 1.8)                                     // 大软光点：体积感
        : exp(-r2 * 8.0 * thin) + exp(-r2 * 2.2) * 0.30 / thin; // 火点：硬核 + 光晕
    // quad 边硬归零窗（验收定点修复）：高斯衰减在 sprite 边不归零，大 glow 点会漏方框边——
    // 速度向两端 0.80 起渐灭、横向 0.60 起渐灭，任何尺寸/拉伸下 quad 边永不可见
    float win = (1.0 - smoothstep(0.80, 1.0, abs(p.x)))
              * (1.0 - smoothstep(0.60, 1.0, abs(p.y)));
    fragColor = vec4(vCol * (m * vAlpha * win), 1.0);
}`;

const BG_UNIFORMS = [
  'uRes', 'uAspect', 'uTime',
  'uStars', 'uStarStretch', 'uMilky',
  'uMoon', 'uMoonPos', 'uMoonR',
  'uFurnace', 'uFurnaceR',
  'uEngine',
  'uShock', 'uShockApex',
  'uBody', 'uBodyPos', 'uBodyLen', 'uBodyMolten', 'uBodyMorph',
  'uPlanetHint',
  'uGroundGlow', 'uGroundPos',
  'uCoreGlow', 'uCorePos',
  'uFlash', 'uFlashPos',
  'uVig',
] as const;

const FIRE_UNIFORMS = [
  'uAspect', 'uDpr', 'uTime',
  'uP0', 'uVel', 'uAcc',
  'uDir', 'uSpread', 'uSpawnR',
  'uSpeed', 'uSpeedVar', 'uInherit',
  'uLife', 'uLifeVar',
  'uSize0', 'uSize1', 'uSizeVar',
  'uGrav', 'uDrag',
  'uTurb', 'uTurbFreq', 'uTurbTime',
  'uCount01', 'uBurst', 'uBurstSpan',
  'uRadial01', 'uRadialSpeed', 'uRadialDrag', 'uRadialLife',
  'uSoft01', 'uSoftSize', 'uSoftGain',
  'uStreak', 'uTwinkle', 'uGain', 'uEmber01',
  'uAlpha',
  'uColHot', 'uColMid', 'uColTail',
] as const;

/* ================================================================
 * 每拍编舞表
 * ================================================================ */

const HOT = COLOR_PRESENCE.whiteHotRgb;
const GOLD = COLOR_PRESENCE.goldRgb;
const AMBER = COLOR_PRESENCE.amberRgb;
const EMBER_TAIL: Vec3 = [0.45, 0.16, 0.04];
const MOLTEN_MID: Vec3 = [1.0, 0.86, 0.55];
const MOLTEN_TAIL: Vec3 = [0.70, 0.35, 0.10];

const FIRE_TABLE: Record<IntroBeatId, FireParams> = {
  // 1 打火石：纯黑中一簇金色火花迸溅（放射 + 微重力下坠 + 强 streak；
  // 大 sprite 让 UV 速度向拉伸显成火丝——丝长上限 = sprite 边长，小点永远只是点）
  spark: {
    origin: [0, -0.02], dir: [0, 1], spread: 2.6, spawnR: 0.012,
    speed: 1.7, speedVar: 0.75, inherit: 0,
    life: 0.65, lifeVar: 0.5, size0: 20, size1: 2.0, sizeVar: 0.4,
    grav: [0, -1.6], drag: 1.2, turb: 0.10, turbFreq: 2.0, turbTime: 1.0,
    count01: 0.8, burst: 1, burstSpan: 0.32,
    radial01: 0, radialSpeed: 0, radialDrag: 0, radialLife: 0.5,
    soft01: 0, softSize: 0, softGain: 0,
    streak: 1.3, twinkle: 0.6, ember01: 0.15, gain: 1.35,
    colHot: HOT, colMid: GOLD, colTail: AMBER,
  },
  // 2 营火：中心偏下，火焰升腾 + 大软光点体积感 + 火星子类（锥形收束，忌圆胖）
  campfire: {
    origin: [0, -0.30], dir: [0, 1], spread: 0.30, spawnR: 0.045,
    speed: 0.62, speedVar: 0.5, inherit: 0,
    life: 1.15, lifeVar: 0.45, size0: 8, size1: 1.6, sizeVar: 0.5,
    grav: [0, 0.35], drag: 0.6, turb: 0.24, turbFreq: 2.6, turbTime: 1.0,
    count01: 0.8, burst: 0, burstSpan: 0,
    radial01: 0, radialSpeed: 0, radialDrag: 0, radialLife: 0.5,
    soft01: 0.22, softSize: 100, softGain: 0.045,
    streak: 0.15, twinkle: 0.35, ember01: 0.12, gain: 1.0,
    colHot: HOT, colMid: GOLD, colTail: EMBER_TAIL,
  },
  // 3 蒸汽机：炉膛圆口内翻滚（宽散布 + 大软点 + 铁环 matte；gain 收敛让铁环纹理显形）
  furnace: {
    origin: [0, -0.03], dir: [0, 1], spread: 0.9, spawnR: 0.10,
    speed: 0.42, speedVar: 0.6, inherit: 0,
    life: 0.95, lifeVar: 0.5, size0: 14, size1: 3, sizeVar: 0.5,
    grav: [0, 0.12], drag: 0.9, turb: 0.42, turbFreq: 3.0, turbTime: 1.2,
    count01: 0.85, burst: 0, burstSpan: 0,
    radial01: 0, radialSpeed: 0, radialDrag: 0, radialLife: 0.5,
    soft01: 0.30, softSize: 130, softGain: 0.05,
    streak: 0.10, twinkle: 0.25, ember01: 0.05, gain: 0.88,
    colHot: HOT, colMid: GOLD, colTail: EMBER_TAIL,
  },
  // 4 内燃机：气缸内燃烧火球（紧致 + 高湍流 + JS 侧脉动 gain；收敛亮度让气缸剖面显形）
  engine: {
    origin: [0, 0.0], dir: [0, 1], spread: 2.8, spawnR: 0.07,
    speed: 0.34, speedVar: 0.6, inherit: 0,
    life: 0.70, lifeVar: 0.4, size0: 14, size1: 4, sizeVar: 0.5,
    grav: [0, 0], drag: 1.3, turb: 0.60, turbFreq: 4.0, turbTime: 1.6,
    count01: 0.75, burst: 0, burstSpan: 0,
    radial01: 0, radialSpeed: 0, radialDrag: 0, radialLife: 0.5,
    soft01: 0.40, softSize: 120, softGain: 0.07,
    streak: 0.05, twinkle: 0.15, ember01: 0, gain: 0.9,
    colHot: HOT, colMid: GOLD, colTail: AMBER,
  },
  // 5 底火出膛：放射环（radial 子类，寿命贯穿全拍 → 环状火焰持续包裹弹身）
  //    + 弹身环火（随加速弹体后掠）
  primer: {
    origin: [0, 0], dir: [-1, 0.10], spread: 0.5, spawnR: 0.03,
    speed: 0.8, speedVar: 0.4, inherit: 0.85,
    life: 0.5, lifeVar: 0.4, size0: 7, size1: 1.2, sizeVar: 0.4,
    grav: [0, 0], drag: 0.8, turb: 0.25, turbFreq: 3.0, turbTime: 1.5,
    count01: 0.9, burst: 1, burstSpan: 1.05,
    radial01: 0.45, radialSpeed: 2.6, radialDrag: 2.0, radialLife: 1.1,
    soft01: 0.10, softSize: 90, softGain: 0.06,
    streak: 0.7, twinkle: 0.3, ember01: 0.10, gain: 1.1,
    colHot: HOT, colMid: GOLD, colTail: AMBER,
  },
  // 6 子弹飞行：火焰尾迹 + 激波锥（相机跟拍，弹体居中）
  flight: {
    origin: [0.26, 0], dir: [-1, 0], spread: 0.14, spawnR: 0.02,
    speed: 1.05, speedVar: 0.35, inherit: 0.1,
    life: 0.85, lifeVar: 0.4, size0: 8, size1: 1.4, sizeVar: 0.4,
    grav: [0, 0.02], drag: 0.4, turb: 0.45, turbFreq: 3.2, turbTime: 2.0,
    count01: 0.9, burst: 0, burstSpan: 0,
    radial01: 0, radialSpeed: 0, radialDrag: 0, radialLife: 0.5,
    soft01: 0.18, softSize: 90, softGain: 0.05,
    streak: 0.85, twinkle: 0.2, ember01: 0.2, gain: 1.05,
    colHot: HOT, colMid: GOLD, colTail: EMBER_TAIL,
  },
  // 7 变形：熔成金色流线（更高湍流 + 熔融色 + 弹体 morph）
  morph: {
    origin: [0.28, 0], dir: [-1, 0], spread: 0.16, spawnR: 0.03,
    speed: 1.0, speedVar: 0.35, inherit: 0.1,
    life: 0.8, lifeVar: 0.4, size0: 10, size1: 1.6, sizeVar: 0.4,
    grav: [0, 0], drag: 0.4, turb: 0.75, turbFreq: 3.6, turbTime: 2.4,
    count01: 0.9, burst: 0, burstSpan: 0,
    radial01: 0, radialSpeed: 0, radialDrag: 0, radialLife: 0.5,
    soft01: 0.22, softSize: 100, softGain: 0.06,
    streak: 0.8, twinkle: 0.15, ember01: 0.15, gain: 1.1,
    colHot: HOT, colMid: MOLTEN_MID, colTail: MOLTEN_TAIL,
  },
  // 8 划月：火箭跨屏金线（低自速 + 零继承 → 尾迹留成线）
  moon: {
    origin: [0, 0], dir: [-1, -0.18], spread: 0.10, spawnR: 0.008,
    speed: 0.3, speedVar: 0.4, inherit: 0.05,
    life: 0.55, lifeVar: 0.3, size0: 6, size1: 1.0, sizeVar: 0.4,
    grav: [0, 0], drag: 0.1, turb: 0.18, turbFreq: 3.0, turbTime: 1.5,
    count01: 0.7, burst: 0, burstSpan: 0,
    radial01: 0, radialSpeed: 0, radialDrag: 0, radialLife: 0.5,
    soft01: 0.10, softSize: 60, softGain: 0.05,
    streak: 0.95, twinkle: 0.1, ember01: 0.1, gain: 1.15,
    colHot: HOT, colMid: GOLD, colTail: AMBER,
  },
  // 9 抵达：金线弧线减速划向行星/星环方向（长尾 + 随 tau 衰减）
  arrival: {
    origin: [0, 0], dir: [-1, -0.3], spread: 0.08, spawnR: 0.006,
    speed: 0.2, speedVar: 0.3, inherit: 0.05,
    life: 1.1, lifeVar: 0.3, size0: 5, size1: 0.8, sizeVar: 0.4,
    grav: [0, 0], drag: 0.1, turb: 0.12, turbFreq: 3.0, turbTime: 1.2,
    count01: 0.5, burst: 0, burstSpan: 0,
    radial01: 0, radialSpeed: 0, radialDrag: 0, radialLife: 0.5,
    soft01: 0.08, softSize: 50, softGain: 0.04,
    streak: 0.95, twinkle: 0.05, ember01: 0.08, gain: 1.05,
    colHot: HOT, colMid: GOLD, colTail: AMBER,
  },
  // 10 落幅：粒子归零，只剩背景暗示与 DOM 淡出
  landing: {
    origin: [0, 0], dir: [0, 1], spread: 0, spawnR: 0,
    speed: 0, speedVar: 0, inherit: 0,
    life: 1, lifeVar: 0, size0: 0, size1: 0, sizeVar: 0,
    grav: [0, 0], drag: 0, turb: 0, turbFreq: 1, turbTime: 1,
    count01: 0, burst: 0, burstSpan: 0,
    radial01: 0, radialSpeed: 0, radialDrag: 0, radialLife: 0.5,
    soft01: 0, softSize: 0, softGain: 0,
    streak: 0, twinkle: 0, ember01: 0, gain: 0,
    colHot: HOT, colMid: GOLD, colTail: AMBER,
  },
};

/* ================================================================
 * 工具函数
 * ================================================================ */

function clamp01(v: number): number {
  return Math.min(1, Math.max(0, v));
}

function sstep01(a: number, b: number, x: number): number {
  const t = clamp01((x - a) / (b - a));
  return t * t * (3 - 2 * t);
}

function easeInOutCubic(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

function easeOutCubic(t: number): number {
  return 1 - Math.pow(1 - t, 3);
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

function lerpVec2(a: Vec2, b: Vec2, t: number): Vec2 {
  return [lerp(a[0], b[0], t), lerp(a[1], b[1], t)];
}

/** 数值微分弹道采样：p = f(t)，v/a 中心差分（h=8ms） */
function trackAt(fn: (t: number) => Vec2, t: number): OriginTrack {
  const h = 0.008;
  const p = fn(t);
  const pf = fn(t + h);
  const pb = fn(Math.max(0, t - h));
  return {
    p,
    v: [(pf[0] - pb[0]) / (2 * h), (pf[1] - pb[1]) / (2 * h)],
    a: [(pf[0] - 2 * p[0] + pb[0]) / (h * h), (pf[1] - 2 * p[1] + pb[1]) / (h * h)],
  };
}

function staticTrack(p: Vec2): OriginTrack {
  return {p, v: [0, 0], a: [0, 0]};
}

/* ================================================================
 * 引擎
 * ================================================================ */

export interface IntroFireOptions {
  /** 每帧进度回调（秒；IntroLayer 据此驱动落幅 DOM 淡出） */
  onProgress?: (t: number) => void;
  /** 播完回调（跳过=快进到落幅淡出，播完同样触发） */
  onComplete?: () => void;
}

export class IntroFire {
  private canvas: HTMLCanvasElement;
  private readonly opts: IntroFireOptions;
  private destroyed = false;
  private completed = false;

  private gl: WebGL2RenderingContext | null = null;
  private bgProg: WebGLProgram | null = null;
  private fireProg: WebGLProgram | null = null;
  private bgVao: WebGLVertexArrayObject | null = null;
  private fireVao: WebGLVertexArrayObject | null = null;
  private bgUni: Record<string, WebGLUniformLocation | null> = {};
  private fireUni: Record<string, WebGLUniformLocation | null> = {};

  private rafId: number | null = null;
  private lastT = 0;
  private tSec = 0;

  private cssW = 0;
  private cssH = 0;
  private dpr = 1;
  private aspect = 1;

  /* 开发工具：?it=<秒> 冻结开场时钟（照抄 blackhole.ts ?t0= 纪律）——
     headless virtual-time 下 WebGL 只呈现早期帧，借此截取任意分拍时刻。 */
  private debugIt: number | null =
    typeof location !== 'undefined'
      ? (() => {
          const v = parseFloat(new URLSearchParams(location.search).get('it') ?? '');
          return Number.isFinite(v) ? v : null;
        })()
      : null;

  constructor(canvas: HTMLCanvasElement, options: IntroFireOptions = {}) {
    this.canvas = canvas;
    this.opts = options;
    this.layout();
    if (!this.initGL()) {
      // WebGL2 不可用：开场无法渲染，直接交还控制权（不黑屏、不阻塞）
      console.warn('[intro] WebGL2 不可用，跳过开场动画');
      queueMicrotask(() => this.opts.onComplete?.());
      return;
    }
    this.resizeGL();
    window.addEventListener('resize', this.resizeHandler);
    document.addEventListener('visibilitychange', this.visibilityHandler);
    this.startLoop();
  }

  /** 跳过：快进到落幅淡出（冻结调试 ?it= 下无效） */
  skip(): void {
    if (this.destroyed || this.completed || this.debugIt !== null) return;
    if (this.tSec < INTRO_LANDING_START) this.tSec = INTRO_LANDING_START;
  }

  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.stopLoop();
    window.removeEventListener('resize', this.resizeHandler);
    document.removeEventListener('visibilitychange', this.visibilityHandler);
    if (this.gl) {
      const ext = this.gl.getExtension('WEBGL_lose_context');
      if (ext) ext.loseContext();
      this.gl = null;
    }
  }

  /* ----------------------------------------------------------------
   * 布局 / GL 初始化（纪律对齐 blackhole.ts）
   * ---------------------------------------------------------------- */

  private layout(): void {
    const rect = this.canvas.getBoundingClientRect();
    this.cssW = Math.max(1, rect.width || window.innerWidth);
    this.cssH = Math.max(1, rect.height || window.innerHeight);
    this.aspect = this.cssW / this.cssH;
    this.dpr = Math.min(window.devicePixelRatio || 1, PERFORMANCE.dprMax);
  }

  private compileShader(type: number, src: string): WebGLShader | null {
    const gl = this.gl;
    if (!gl) return null;
    const s = gl.createShader(type);
    if (!s) return null;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      console.warn('[intro] shader 编译失败:', gl.getShaderInfoLog(s));
      gl.deleteShader(s);
      return null;
    }
    return s;
  }

  private linkProgram(vsSrc: string, fsSrc: string): WebGLProgram | null {
    const gl = this.gl;
    if (!gl) return null;
    const vs = this.compileShader(gl.VERTEX_SHADER, vsSrc);
    const fs = this.compileShader(gl.FRAGMENT_SHADER, fsSrc);
    if (!vs || !fs) return null;
    const prog = gl.createProgram();
    if (!prog) return null;
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      console.warn('[intro] program 链接失败:', gl.getProgramInfoLog(prog));
      return null;
    }
    return prog;
  }

  private initGL(): boolean {
    try {
      this.gl = this.canvas.getContext('webgl2', {antialias: false, alpha: false, depth: false, stencil: false});
    } catch {
      this.gl = null;
    }
    const gl = this.gl;
    if (!gl) return false;
    this.bgProg = this.linkProgram(BG_VERT, BG_FRAG);
    this.fireProg = this.linkProgram(FIRE_VERT, FIRE_FRAG);
    if (!this.bgProg || !this.fireProg) return false;

    // 背景 quad VAO（绘制时必须重新绑定——默认 VAO 无属性数组，顶点是常量 (0,0) 会黑屏）
    const bgVao = gl.createVertexArray();
    gl.bindVertexArray(bgVao);
    const quad = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    this.bgVao = bgVao;

    // 粒子 seed VAO（确定性 hash，刷新不重排）
    const fireVao = gl.createVertexArray();
    gl.bindVertexArray(fireVao);
    const seedBuf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, seedBuf);
    const seeds = new Float32Array(MAX_PARTICLES * 4);
    let s = 1234567;
    for (let i = 0; i < seeds.length; i++) {
      // xorshift32 确定性伪随机
      s ^= s << 13; s ^= s >>> 17; s ^= s << 5;
      seeds[i] = ((s >>> 0) % 100000) / 100000;
    }
    gl.bufferData(gl.ARRAY_BUFFER, seeds, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 4, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);
    this.fireVao = fireVao;

    gl.useProgram(this.bgProg);
    for (const n of BG_UNIFORMS) this.bgUni[n] = gl.getUniformLocation(this.bgProg, n);
    gl.useProgram(this.fireProg);
    for (const n of FIRE_UNIFORMS) this.fireUni[n] = gl.getUniformLocation(this.fireProg, n);
    return true;
  }

  private resizeGL(): void {
    const gl = this.gl;
    if (!gl) return;
    this.canvas.width = Math.max(1, Math.round(this.cssW * this.dpr));
    this.canvas.height = Math.max(1, Math.round(this.cssH * this.dpr));
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
  }

  private resizeHandler = (): void => {
    if (this.destroyed) return;
    this.layout();
    this.resizeGL();
  };

  private visibilityHandler = (): void => {
    if (document.hidden) this.stopLoop();
    else this.startLoop();
  };

  /* ----------------------------------------------------------------
   * 每拍动态编舞（moving origin 弹道 + 脉动 + 背景特征）
   * ---------------------------------------------------------------- */

  /** 拍 id + 本地时间 → 弹道 p/v/a 与动态修正（gain/count 乘子） */
  private dynamics(beat: BeatWeight): {track: OriginTrack; gainMul: number; countMul: number} {
    const lt = beat.localT;
    const tau = beat.tau;
    const A = this.aspect;
    switch (beat.id) {
      case 'spark':
        return {track: staticTrack(FIRE_TABLE.spark.origin), gainMul: 1, countMul: 1};
      case 'campfire':
        return {track: staticTrack(FIRE_TABLE.campfire.origin), gainMul: 1, countMul: 1};
      case 'furnace':
        return {track: staticTrack(FIRE_TABLE.furnace.origin), gainMul: 1, countMul: 1};
      case 'engine':
        // 点火脉动（概念图 beat-04：火光脉动）
        return {
          track: staticTrack(FIRE_TABLE.engine.origin),
          gainMul: 1 + 0.30 * Math.sin(lt * 13),
          countMul: 1,
        };
      case 'primer': {
        // 子弹加速旋出：x = 0.34·s²（s 截断在拍内）
        const dur = 1.2;
        const fn = (t: number): Vec2 => [0.34 * Math.pow(Math.min(Math.max(t, 0), dur) / dur, 2), 0];
        return {track: trackAt(fn, lt), gainMul: 1, countMul: 1};
      }
      case 'flight': {
        const fn = (t: number): Vec2 => [0.26 + 0.008 * Math.sin(t * 2.6), 0.010 * Math.sin(t * 2.1)];
        return {track: trackAt(fn, lt), gainMul: 1, countMul: 1};
      }
      case 'morph': {
        const fn = (t: number): Vec2 => [0.28 + 0.008 * Math.sin(t * 2.4), 0.010 * Math.sin(t * 1.9)];
        return {track: trackAt(fn, lt), gainMul: 1, countMul: 1};
      }
      case 'moon': {
        // 火箭跨屏：左下 → 右上掠月，轻微上弧
        const dur = 2.0;
        const p0: Vec2 = [-0.95 * A, -0.55];
        const p1: Vec2 = [0.85 * A, 0.38];
        const fn = (t: number): Vec2 => {
          const s = clamp01(t / dur);
          return [
            lerp(p0[0], p1[0], s),
            lerp(p0[1], p1[1], s) + 0.22 * Math.sin(Math.PI * s),
          ];
        };
        return {track: trackAt(fn, lt), gainMul: 1, countMul: 1};
      }
      case 'arrival': {
        // 金线弧线减速（ease-out = 抵达感），弯向行星/星环方向（左下环平原）
        const dur = 2.5;
        const P0: Vec2 = [0.80 * A, 0.45];
        const C1: Vec2 = [0.15 * A, 0.60];
        const C2: Vec2 = [-0.35 * A, 0.25];
        const P1: Vec2 = [-0.62 * A, -0.30];
        const fn = (t: number): Vec2 => {
          const s = easeOutCubic(clamp01(t / dur));
          const u = 1 - s;
          return [
            u * u * u * P0[0] + 3 * u * u * s * C1[0] + 3 * u * s * s * C2[0] + s * s * s * P1[0],
            u * u * u * P0[1] + 3 * u * u * s * C1[1] + 3 * u * s * s * C2[1] + s * s * s * P1[1],
          ];
        };
        return {
          track: trackAt(fn, lt),
          gainMul: 1 - 0.55 * sstep01(0.55, 1, tau),
          countMul: 1 - 0.6 * tau,
        };
      }
      case 'landing':
        return {track: staticTrack([0, 0]), gainMul: 0, countMul: 0};
    }
  }

  /** 拍 id + 弹道 → 背景 matte 参数（静态表 + 动态覆盖） */
  private bgFor(beat: BeatWeight, track: OriginTrack): BgParams {
    const A = this.aspect;
    const lt = beat.localT;
    const tau = beat.tau;
    const base: BgParams = {
      stars: 0, starStretch: 0, milky: 0,
      moon: 0, moonPos: [-0.42 * A, 0.38], moonR: 0.52,
      furnace: 0, furnaceR: 0.40,
      engine: 0,
      shock: 0, shockApex: [0.46, 0],
      body: 0, bodyPos: [0.32, 0], bodyLen: 0.15, bodyMolten: 0, bodyMorph: 0,
      planetHint: 0,
      groundGlow: 0, groundPos: [0, -0.55],
      coreGlow: 0, corePos: [0, 0],
      flash: 0, flashPos: [0, 0],
      vig: 0.42,
    };
    switch (beat.id) {
      case 'spark':
        base.flash = 0.9 * Math.exp(-Math.max(lt - 0.05, 0) * 5);
        base.flashPos = track.p;
        base.coreGlow = 0.45 * Math.exp(-Math.max(lt - 0.05, 0) * 4) + 0.34;
        base.corePos = track.p;
        break;
      case 'campfire':
        base.groundGlow = 0.55;
        base.coreGlow = 0.5;
        base.corePos = [track.p[0], track.p[1] + 0.10];
        break;
      case 'furnace':
        base.furnace = 1;
        base.coreGlow = 0.55;
        base.corePos = track.p;
        break;
      case 'engine':
        base.engine = 1;
        base.coreGlow = 0.7 * (1 + 0.25 * Math.sin(lt * 13));
        base.corePos = track.p;
        base.flash = 0.5 * Math.exp(-Math.max(lt - 0.1, 0) * 6);
        base.flashPos = track.p;
        break;
      case 'primer': {
        base.flash = 1.1 * Math.exp(-Math.max(lt - 0.05, 0) * 5);
        base.flashPos = track.p;
        base.coreGlow = 0.9;
        base.corePos = track.p;
        const bw = sstep01(0.2, 0.5, tau);
        base.body = bw;
        base.bodyPos = [track.p[0] + 0.075, track.p[1]];
        base.shock = 0.5 * sstep01(0.55, 1, tau);
        base.shockApex = [track.p[0] + 0.20, track.p[1]];
        break;
      }
      case 'flight':
        base.stars = 0.8;
        base.starStretch = 1;
        base.body = 1;
        base.bodyPos = [track.p[0] + 0.075, track.p[1]];
        base.shock = 1;
        base.shockApex = [track.p[0] + 0.22, track.p[1]];
        base.coreGlow = 0.9;
        base.corePos = [track.p[0] + 0.04, track.p[1]];
        break;
      case 'morph': {
        base.stars = 0.8;
        base.starStretch = 1;
        const m = easeInOutCubic(tau);
        base.body = 1;
        base.bodyLen = 0.15 + 0.05 * m;
        base.bodyMorph = m;
        base.bodyMolten = 0.15 + 0.6 * tau;
        base.bodyPos = [track.p[0] + 0.075 + 0.04 * m, track.p[1]];
        base.shock = 1 - 0.7 * tau;
        base.shockApex = [track.p[0] + 0.26, track.p[1]];
        base.coreGlow = 0.75;
        base.corePos = [track.p[0] + 0.05, track.p[1]];
        break;
      }
      case 'moon':
        base.stars = 1;
        base.starStretch = 0.25;
        base.milky = 1;
        base.moon = sstep01(0, 0.35, tau);
        base.body = 1 - sstep01(0, 0.2, tau);
        base.bodyPos = track.p;
        base.bodyLen = 0.19;
        base.bodyMorph = 1;
        base.coreGlow = 1.3;
        base.corePos = track.p;
        break;
      case 'arrival':
        base.stars = 1;
        base.starStretch = 0.1;
        base.milky = 0.35;
        base.planetHint = sstep01(0.15, 0.85, tau);
        base.coreGlow = 1.1 * (1 - 0.75 * tau);
        base.corePos = track.p;
        break;
      case 'landing':
        base.stars = 1;
        base.milky = 0.3;
        base.planetHint = 1;
        break;
    }
    return base;
  }

  /* ----------------------------------------------------------------
   * 渲染
   * ---------------------------------------------------------------- */

  private mixBg(a: BgParams, aw: number, b: BgParams, bw: number): BgParams {
    const t = bw / (aw + bw);
    return {
      stars: lerp(a.stars, b.stars, t),
      starStretch: lerp(a.starStretch, b.starStretch, t),
      milky: lerp(a.milky, b.milky, t),
      moon: lerp(a.moon, b.moon, t),
      moonPos: lerpVec2(a.moonPos, b.moonPos, t),
      moonR: lerp(a.moonR, b.moonR, t),
      furnace: lerp(a.furnace, b.furnace, t),
      furnaceR: lerp(a.furnaceR, b.furnaceR, t),
      engine: lerp(a.engine, b.engine, t),
      shock: lerp(a.shock, b.shock, t),
      shockApex: lerpVec2(a.shockApex, b.shockApex, t),
      body: lerp(a.body, b.body, t),
      bodyPos: lerpVec2(a.bodyPos, b.bodyPos, t),
      bodyLen: lerp(a.bodyLen, b.bodyLen, t),
      bodyMolten: lerp(a.bodyMolten, b.bodyMolten, t),
      bodyMorph: lerp(a.bodyMorph, b.bodyMorph, t),
      planetHint: lerp(a.planetHint, b.planetHint, t),
      groundGlow: lerp(a.groundGlow, b.groundGlow, t),
      groundPos: lerpVec2(a.groundPos, b.groundPos, t),
      coreGlow: lerp(a.coreGlow, b.coreGlow, t),
      corePos: lerpVec2(a.corePos, b.corePos, t),
      flash: lerp(a.flash, b.flash, t),
      flashPos: lerpVec2(a.flashPos, b.flashPos, t),
      vig: lerp(a.vig, b.vig, t),
    };
  }

  private drawBg(bg: BgParams, tGlobal: number): void {
    const gl = this.gl;
    if (!gl || !this.bgProg) return;
    const u = this.bgUni;
    gl.useProgram(this.bgProg);
    gl.bindVertexArray(this.bgVao);
    gl.disable(gl.BLEND);
    gl.uniform2f(u.uRes, this.canvas.width, this.canvas.height);
    gl.uniform1f(u.uAspect, this.aspect);
    gl.uniform1f(u.uTime, tGlobal);
    gl.uniform1f(u.uStars, bg.stars);
    gl.uniform1f(u.uStarStretch, bg.starStretch);
    gl.uniform1f(u.uMilky, bg.milky);
    gl.uniform1f(u.uMoon, bg.moon);
    gl.uniform2f(u.uMoonPos, bg.moonPos[0], bg.moonPos[1]);
    gl.uniform1f(u.uMoonR, bg.moonR);
    gl.uniform1f(u.uFurnace, bg.furnace);
    gl.uniform1f(u.uFurnaceR, bg.furnaceR);
    gl.uniform1f(u.uEngine, bg.engine);
    gl.uniform1f(u.uShock, bg.shock);
    gl.uniform2f(u.uShockApex, bg.shockApex[0], bg.shockApex[1]);
    gl.uniform1f(u.uBody, bg.body);
    gl.uniform2f(u.uBodyPos, bg.bodyPos[0], bg.bodyPos[1]);
    gl.uniform1f(u.uBodyLen, bg.bodyLen);
    gl.uniform1f(u.uBodyMolten, bg.bodyMolten);
    gl.uniform1f(u.uBodyMorph, bg.bodyMorph);
    gl.uniform1f(u.uPlanetHint, bg.planetHint);
    gl.uniform1f(u.uGroundGlow, bg.groundGlow);
    gl.uniform2f(u.uGroundPos, bg.groundPos[0], bg.groundPos[1]);
    gl.uniform1f(u.uCoreGlow, bg.coreGlow);
    gl.uniform2f(u.uCorePos, bg.corePos[0], bg.corePos[1]);
    gl.uniform1f(u.uFlash, bg.flash);
    gl.uniform2f(u.uFlashPos, bg.flashPos[0], bg.flashPos[1]);
    gl.uniform1f(u.uVig, bg.vig);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  private drawFire(p: FireParams, track: OriginTrack, alpha: number, gainMul: number, countMul: number, localT: number): void {
    const gl = this.gl;
    if (!gl || !this.fireProg) return;
    const count = p.count01 * countMul;
    if (count <= 0.001 || alpha <= 0.001) return;
    const u = this.fireUni;
    gl.useProgram(this.fireProg);
    gl.bindVertexArray(this.fireVao);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE);
    gl.uniform1f(u.uAspect, this.aspect);
    gl.uniform1f(u.uDpr, this.dpr);
    gl.uniform1f(u.uTime, localT);
    gl.uniform2f(u.uP0, track.p[0], track.p[1]);
    gl.uniform2f(u.uVel, track.v[0], track.v[1]);
    gl.uniform2f(u.uAcc, track.a[0], track.a[1]);
    gl.uniform2f(u.uDir, p.dir[0], p.dir[1]);
    gl.uniform1f(u.uSpread, p.spread);
    gl.uniform1f(u.uSpawnR, p.spawnR);
    gl.uniform1f(u.uSpeed, p.speed);
    gl.uniform1f(u.uSpeedVar, p.speedVar);
    gl.uniform1f(u.uInherit, p.inherit);
    gl.uniform1f(u.uLife, p.life);
    gl.uniform1f(u.uLifeVar, p.lifeVar);
    gl.uniform1f(u.uSize0, p.size0);
    gl.uniform1f(u.uSize1, p.size1);
    gl.uniform1f(u.uSizeVar, p.sizeVar);
    gl.uniform2f(u.uGrav, p.grav[0], p.grav[1]);
    gl.uniform1f(u.uDrag, p.drag);
    gl.uniform1f(u.uTurb, p.turb);
    gl.uniform1f(u.uTurbFreq, p.turbFreq);
    gl.uniform1f(u.uTurbTime, p.turbTime);
    gl.uniform1f(u.uCount01, count);
    gl.uniform1f(u.uBurst, p.burst);
    gl.uniform1f(u.uBurstSpan, p.burstSpan);
    gl.uniform1f(u.uRadial01, p.radial01);
    gl.uniform1f(u.uRadialSpeed, p.radialSpeed);
    gl.uniform1f(u.uRadialDrag, p.radialDrag);
    gl.uniform1f(u.uRadialLife, p.radialLife);
    gl.uniform1f(u.uSoft01, p.soft01);
    gl.uniform1f(u.uSoftSize, p.softSize);
    gl.uniform1f(u.uSoftGain, p.softGain);
    gl.uniform1f(u.uStreak, p.streak);
    gl.uniform1f(u.uTwinkle, p.twinkle);
    gl.uniform1f(u.uGain, p.gain * gainMul);
    gl.uniform1f(u.uEmber01, p.ember01);
    gl.uniform1f(u.uAlpha, alpha);
    gl.uniform3f(u.uColHot, p.colHot[0], p.colHot[1], p.colHot[2]);
    gl.uniform3f(u.uColMid, p.colMid[0], p.colMid[1], p.colMid[2]);
    gl.uniform3f(u.uColTail, p.colTail[0], p.colTail[1], p.colTail[2]);
    gl.drawArrays(gl.POINTS, 0, MAX_PARTICLES);
    gl.disable(gl.BLEND);
  }

  private render(t: number): void {
    const gl = this.gl;
    if (!gl) return;
    const beats = activeBeats(t);
    if (!beats.length) return;

    const tracks = beats.map((b) => this.dynamics(b));
    const bgs = beats.map((b, i) => this.bgFor(b, tracks[i].track));
    const mixed = bgs.length === 1 ? bgs[0] : this.mixBg(bgs[0], beats[0].weight, bgs[1], beats[1].weight);
    this.drawBg(mixed, t);

    for (let i = 0; i < beats.length; i++) {
      const b = beats[i];
      const dyn = tracks[i];
      this.drawFire(FIRE_TABLE[b.id], dyn.track, b.weight, dyn.gainMul, dyn.countMul, b.localT);
    }
  }

  private loop = (now: number): void => {
    this.rafId = requestAnimationFrame(this.loop);
    const dt = Math.min((now - this.lastT) / 1000, PERFORMANCE.dtClampSec);
    this.lastT = now;
    this.tSec += dt;
    // 开发工具：?it=<秒> 冻结时钟，供 headless 帧差验证；title 暴露内部状态供 --dump-dom 断言
    if (this.debugIt !== null) {
      this.tSec = this.debugIt;
      document.title = `intro t=${this.tSec.toFixed(2)}`;
    }
    this.opts.onProgress?.(this.tSec);
    this.render(this.tSec);
    if (!this.completed && this.debugIt === null && this.tSec >= INTRO_TOTAL) {
      this.completed = true;
      this.opts.onComplete?.();
    }
  };

  private startLoop(): void {
    if (this.destroyed || this.rafId !== null) return;
    this.lastT = performance.now();
    this.rafId = requestAnimationFrame(this.loop);
  }

  private stopLoop(): void {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
  }
}
