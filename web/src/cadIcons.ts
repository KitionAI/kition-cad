/**
 * Kition CAD's own command icons, for the commands no Tessera icon tells apart: the ways to draw a
 * circle, an arc, an ellipse, a point and a revision cloud, a spline by control vertices, paragraph
 * text, a gradient, copy with a base point, break at a point and zoom to the previous view. Original
 * drawings by the Kition authors (docs/attribution.md): plain geometry on Tessera's 24 px grid, in
 * its style (1.5 px strokes, round ends, dots filled); the points a command asks for are dots, a
 * centre a small cross, the order of the first two points an arrow from the first to the second.
 */
import { createIcon, type IconComponent } from '@kition/tessera/icons/react'

const n = (value: number) => Number(value.toFixed(2))
type P = [number, number]

/** A point at `degrees` (counterclockwise from +x, as in a drawing) on a circle; y grows down. */
const at = ([cx, cy]: P, r: number, degrees: number): P => [n(cx + r * Math.cos((degrees * Math.PI) / 180)), n(cy - r * Math.sin((degrees * Math.PI) / 180))]
const line = (...points: P[]) => points.map(([x, y], i) => `${i ? 'L' : 'M'}${n(x)} ${n(y)}`).join(' ')
const closed = (...points: P[]) => `${line(...points)} Z`
const circle = ([cx, cy]: P, r: number) => `M${n(cx - r)} ${n(cy)} a${n(r)} ${n(r)} 0 1 0 ${n(2 * r)} 0 a${n(r)} ${n(r)} 0 1 0 ${n(-2 * r)} 0`
const ellipse = ([cx, cy]: P, rx: number, ry: number) => `M${n(cx - rx)} ${n(cy)} a${rx} ${ry} 0 1 0 ${n(2 * rx)} 0 a${rx} ${ry} 0 1 0 ${n(-2 * rx)} 0`
/** An arc counterclockwise (as in a drawing) from `from` to `to` degrees. */
const arc = (center: P, r: number, from: number, to: number) => {
  const [sx, sy] = at(center, r, from)
  const [ex, ey] = at(center, r, to)
  const large = ((to - from + 360) % 360) > 180 ? 1 : 0
  return `M${sx} ${sy} A${n(r)} ${n(r)} 0 ${large} 0 ${ex} ${ey}`
}
const dot = (p: P, r = 1.5) => circle(p, r)
const cross = ([x, y]: P, s = 1.8) => `${line([x - s, y], [x + s, y])} ${line([x, y - s], [x, y + s])}`
/** A line from `from` to `tip` with an arrowhead at `tip`. */
const arrow = (from: P, tip: P, head = 3) => {
  const angle = Math.atan2(tip[1] - from[1], tip[0] - from[0])
  const wing = (turn: number): P => [tip[0] - head * Math.cos(angle + turn), tip[1] - head * Math.sin(angle + turn)]
  return `${line(from, tip)} ${line(wing(0.5), tip, wing(-0.5))}`
}
/** A scalloped outline along a polygon listed clockwise (on screen): `bumps` arcs per side, bulging out. */
const scallops = (points: P[], bumps: number) => {
  let path = `M${n(points[0][0])} ${n(points[0][1])}`
  points.forEach((p, i) => {
    const q = points[(i + 1) % points.length]
    for (let k = 1; k <= bumps; k += 1) {
      const x = p[0] + ((q[0] - p[0]) * k) / bumps
      const y = p[1] + ((q[1] - p[1]) * k) / bumps
      const r = Math.hypot(q[0] - p[0], q[1] - p[1]) / bumps / 2
      path += ` A${n(r)} ${n(r)} 0 0 1 ${n(x)} ${n(y)}`
    }
  })
  return path
}

const icon = (name: string, paths: string[], fills: string[] = []): IconComponent => createIcon({ name: `cad-${name}`, family: 'tool', paths, fills, accents: [] })

// Circles: centre (12, 12), radius 7.
const C: P = [12, 12]
export const CircleCenterDiameterIcon = icon('circle-center-diameter', [circle(C, 7), line([5, 12], [19, 12])], [dot(C, 1.2)])
export const CircleTwoPointsIcon = icon('circle-two-points', [circle(C, 7)], [dot([5, 12]), dot([19, 12])])
export const CircleThreePointsIcon = icon('circle-three-points', [circle(C, 7)], [dot(at(C, 7, 90)), dot(at(C, 7, 210)), dot(at(C, 7, 330))])
/** Tangent to a floor and a wall, with its radius. */
export const CircleTanTanRadiusIcon = icon('circle-tan-tan-radius', [line([3, 3], [3, 21], [21, 21]), circle([9, 15], 6), line([9, 15], at([9, 15], 6, 45))], [dot([9, 15], 1)])
/** Inscribed in a triangle: its incircle, computed. */
const TRIANGLE: P[] = [[3, 20.5], [21, 20.5], [12, 3.5]]
const side = (a: P, b: P) => Math.hypot(a[0] - b[0], a[1] - b[1])
const incircle = (): { center: P; r: number } => {
  const [A, B, Cc] = TRIANGLE
  const a = side(B, Cc), b = side(Cc, A), c = side(A, B)
  const s = a + b + c
  const center: P = [(a * A[0] + b * B[0] + c * Cc[0]) / s, (a * A[1] + b * B[1] + c * Cc[1]) / s]
  const area = Math.abs((B[0] - A[0]) * (Cc[1] - A[1]) - (Cc[0] - A[0]) * (B[1] - A[1])) / 2
  return { center, r: area / (s / 2) }
}
const IN = incircle()
export const CircleTanTanTanIcon = icon('circle-tan-tan-tan', [closed(...TRIANGLE), circle(IN.center, IN.r)])

// Arcs: centre (12, 18), radius 10, counterclockwise from 20° (start, right) to 160° (end, left).
const AC: P = [12, 18]
const S = at(AC, 10, 20)
const E = at(AC, 10, 160)
const M = at(AC, 10, 90)
const ARC = arc(AC, 10, 20, 160)
/** The angle the arc spans, marked at its centre. */
const WEDGE = arc(AC, 3.5, 20, 160)
/** From the start point to the centre: the start comes first. */
const START_TO_CENTER = arrow(S, [AC[0] + 1.2, AC[1] - 0.4], 2.6)
/** From the centre to the start point: the centre comes first. */
const CENTER_TO_START = arrow(AC, [S[0] - 1.4, S[1] + 0.5], 2.6)
const CHORD = line(S, E)
export const ArcThreePointsIcon = icon('arc-three-points', [ARC], [dot(S), dot(M), dot(E)])
export const ArcStartCenterEndIcon = icon('arc-start-center-end', [ARC, START_TO_CENTER], [dot(S), dot(E)])
export const ArcStartCenterAngleIcon = icon('arc-start-center-angle', [ARC, START_TO_CENTER, WEDGE], [dot(S)])
export const ArcStartCenterLengthIcon = icon('arc-start-center-length', [ARC, START_TO_CENTER, CHORD], [dot(S)])
export const ArcCenterStartEndIcon = icon('arc-center-start-end', [ARC, CENTER_TO_START], [dot(AC, 1.2), dot(E)])
export const ArcCenterStartAngleIcon = icon('arc-center-start-angle', [ARC, CENTER_TO_START, WEDGE], [dot(AC, 1.2)])
export const ArcCenterStartLengthIcon = icon('arc-center-start-length', [ARC, CENTER_TO_START, CHORD], [dot(AC, 1.2)])
export const ArcStartEndAngleIcon = icon('arc-start-end-angle', [ARC, line(S, AC, E), WEDGE], [dot(S), dot(E)])
/** The direction at the start: the tangent there. */
const TANGENT_END: P = [n(S[0] - 7 * Math.sin((20 * Math.PI) / 180)), n(S[1] - 7 * Math.cos((20 * Math.PI) / 180))]
export const ArcStartEndDirectionIcon = icon('arc-start-end-direction', [ARC, arrow(S, TANGENT_END, 2.8)], [dot(S), dot(E)])
export const ArcStartEndRadiusIcon = icon('arc-start-end-radius', [ARC, line(AC, M), cross(AC, 1.2)], [dot(S), dot(E)])
/** Continued from the last line, tangent to it. */
const BEFORE: P = [n(S[0] + 7 * Math.sin((20 * Math.PI) / 180)), n(S[1] + 7 * Math.cos((20 * Math.PI) / 180))]
export const ArcContinueIcon = icon('arc-continue', [line(BEFORE, S), ARC], [dot(S)])

// Ellipses: centre (12, 12), axes 9 and 5.
export const EllipseAxisEndIcon = icon('ellipse-axis-end', [ellipse(C, 9, 5), line([3, 12], [21, 12])], [dot([3, 12]), dot([21, 12])])
export const EllipseArcIcon = icon('ellipse-arc', [`M3 13 A9 5 0 0 1 21 13`, `M3 13 A9 5 0 0 0 7 16.6`], [dot([3, 13]), dot([21, 13])])

// Points.
export const PointIcon = icon('point', [line([12, 4], [12, 8.5]), line([12, 15.5], [12, 20]), line([4, 12], [8.5, 12]), line([15.5, 12], [20, 12])], [dot(C, 1.6)])
export const PointMultipleIcon = icon('point-multiple', [cross([6, 7], 2.2), cross([18, 9], 2.2), cross([8, 18], 2.2), cross([17, 18], 2.2)], [dot([6, 7], 1), dot([18, 9], 1), dot([8, 18], 1), dot([17, 18], 1)])
/** A line cut into equal parts. */
export const PointDivideIcon = icon('point-divide', [line([3, 15], [21, 15]), line([3, 12], [3, 18]), line([21, 12], [21, 18])], [dot([7.5, 15]), dot([12, 15]), dot([16.5, 15])])
/** A line marked at a set length from its start, the length measured above it. */
export const PointMeasureIcon = icon('point-measure', [line([3, 16], [21, 16]), line([3, 8], [9, 8]), line([3, 6], [3, 10]), line([9, 6], [9, 10])], [dot([9, 16]), dot([15, 16]), dot([3, 16], 1)])

// Spline by its control vertices: the frame thin lines, the vertices small squares.
const CV: P[] = [[3, 19], [8, 5], [15, 20], [21, 5]]
const square = ([x, y]: P, h = 1.4) => closed([x - h, y - h], [x + h, y - h], [x + h, y + h], [x - h, y + h])
export const SplineControlVerticesIcon = icon('spline-control-vertices', [`M3 19 C8 5 15 20 21 5`, ...CV.slice(1).map((p, i) => line(CV[i], p)), ...CV.map((p) => square(p))])

// Revision clouds by outline.
export const RevisionCloudRectangularIcon = icon('revision-cloud-rectangular', [scallops([[4, 6], [20, 6], [20, 18], [4, 18]], 3)])
export const RevisionCloudPolygonalIcon = icon('revision-cloud-polygonal', [scallops([[12, 3.5], [21, 19], [3, 19]], 3)])

/** Paragraph text: a T over lines of text. */
export const ParagraphTextIcon = icon('paragraph-text', [line([4, 4], [12, 4]), line([8, 4], [8, 11]), line([14, 6], [20, 6]), line([14, 10], [20, 10]), line([4, 15], [20, 15]), line([4, 19], [15, 19])])
/** A gradient: a frame with lines closer and closer together. */
export const GradientIcon = icon('gradient', [closed([3, 5], [21, 5], [21, 19], [3, 19]), ...[7, 10.5, 13.5, 16, 18, 19.5].map((x) => line([x, 5], [x, 19]))])
/** Copy with a base point: two sheets and the point they are picked up by. */
export const CopyBasePointIcon = icon('copy-base-point', [closed([8, 3], [20, 3], [20, 15], [8, 15]), line([5, 7], [4, 7], [4, 21], [16, 21], [16, 18]), cross([8, 15], 2.4)])
/** Break at a point: a line in two pieces meeting at the point. */
export const BreakAtPointIcon = icon('break-at-point', [line([3, 17], [10.2, 12.8]), line([13.8, 11.2], [21, 7]), line([10, 8], [14, 16])], [dot(C, 1.3)])
/** Zoom previous: a lens with an arrow back. */
export const ZoomPreviousIcon = icon('zoom-previous', [circle([10, 10], 6.5), line([14.8, 14.8], [21, 21]), arrow([13, 10], [7, 10], 2.6)])
