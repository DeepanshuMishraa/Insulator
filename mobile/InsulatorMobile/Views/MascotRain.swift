import CoreMotion
import SwiftUI

/// Something solid the mascots land on, in the rain's coordinate space.
struct RainCollider: Equatable {
    var rect: CGRect
    var radius: CGFloat
}

/// Mascots raining down from the top of the screen, piling on `colliders`
/// and the screen's foot, and sliding about as the phone tilts. Touches pass
/// through. Nothing shows with Reduce Motion on.
struct MascotRain: View {
    let colliders: [RainCollider]

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var world = RainWorld()

    var body: some View {
        if !reduceMotion {
            GeometryReader { proxy in
                TimelineView(.animation) { context in
                    Canvas { canvas, size in
                        world.step(to: context.date, size: size, colliders: colliders)
                        for body in world.bodies where body.dropped {
                            let cell = body.size / 8
                            var local = canvas
                            local.translateBy(x: body.x, y: body.y)
                            local.rotate(by: .radians(body.angle))
                            for run in body.sprite.runs(talking: false) {
                                local.fill(
                                    Path(CGRect(
                                        x: run.minX * cell - body.size / 2,
                                        y: run.minY * cell - body.size / 2,
                                        width: run.width * cell,
                                        height: cell
                                    )),
                                    with: .color(body.color)
                                )
                            }
                        }
                    }
                }
                .onAppear { world.start(width: proxy.size.width) }
                .onDisappear { world.stop() }
            }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        }
    }
}

/// Circles under gravity, bouncing off the screen's edges, each other and
/// rounded rects. Stepped once per frame on the main thread.
@MainActor
final class RainWorld {
    struct Body {
        var x: CGFloat
        var y: CGFloat
        var vx: CGFloat
        var vy: CGFloat
        var angle: CGFloat
        var spin: CGFloat
        var radius: CGFloat
        var size: CGFloat
        var dropAt: Double
        var dropped = false
        var entered = false
        let sprite: ProjectMascotSprite
        let color: Color
    }

    private(set) var bodies: [Body] = []
    private var started: Date?
    private var last: Date?
    private var gravity = CGVector(dx: 0, dy: 1)
    private let motion = CMMotionManager()

    private static let count = 18
    private static let g: CGFloat = 2200
    private static let bounce: CGFloat = 0.35
    private static let friction: CGFloat = 0.04
    private static let grip: CGFloat = 0.25
    private static let pairGrip: CGFloat = 0.1
    private static let maxSpin: CGFloat = 12
    private static let substeps = 3
    private static let passes = 4

    func start(width: CGFloat) {
        guard bodies.isEmpty else { return }
        let colors = Array(ProjectColors.all.dropFirst())
        bodies = (0..<Self.count).map { index in
            let size = CGFloat(Int.random(in: 26...36))
            let radius = size * 0.36
            return Body(
                x: .random(in: radius...max(radius, width - radius)),
                y: -size - .random(in: 0...120),
                vx: .random(in: -60...60),
                vy: .random(in: 0...200),
                angle: .random(in: -0.6...0.6),
                spin: .random(in: -3...3),
                radius: radius,
                size: size,
                // Uneven: bunched toward the start, with stragglers.
                dropAt: 2.6 * pow(Double.random(in: 0...1), 1.6),
                sprite: ProjectMascotSprite.all[index % ProjectMascotSprite.all.count],
                color: colors.randomElement() ?? .white
            )
        }
        guard motion.isAccelerometerAvailable else { return }
        motion.accelerometerUpdateInterval = 1.0 / 60
        motion.startAccelerometerUpdates(to: .main) { [weak self] data, _ in
            guard let data else { return }
            MainActor.assumeIsolated { self?.tilt(x: data.acceleration.x, y: data.acceleration.y) }
        }
    }

    func stop() {
        motion.stopAccelerometerUpdates()
    }

    /// Gravity on the screen from an accelerometer reading in g. Lying flat,
    /// a little pull down is kept so they still settle.
    private func tilt(x: Double, y: Double) {
        let gx = min(3, max(-3, x))
        let gy = min(3, max(-3, -y))
        let strength = (gx * gx + gy * gy).squareRoot()
        gravity = CGVector(dx: gx, dy: gy + max(0, 0.35 - strength))
    }

    func step(to date: Date, size: CGSize, colliders: [RainCollider]) {
        let start = started ?? date
        started = start
        // Held short after a stall, so nothing tunnels through a button.
        let dt = min(date.timeIntervalSince(last ?? date), 1.0 / 30)
        last = date
        guard dt > 0 else { return }
        let time = date.timeIntervalSince(start)
        let h = CGFloat(dt) / CGFloat(Self.substeps)
        for _ in 0..<Self.substeps {
            for index in bodies.indices {
                if !bodies[index].dropped {
                    if time < bodies[index].dropAt { continue }
                    bodies[index].dropped = true
                }
                // Still above the screen: always pulled in.
                let gy = bodies[index].entered ? gravity.dy : max(gravity.dy, 0.4)
                bodies[index].vx = (bodies[index].vx + gravity.dx * Self.g * h) * 0.9995
                bodies[index].vy = (bodies[index].vy + gy * Self.g * h) * 0.9995
                bodies[index].spin *= 0.998
                bodies[index].x += bodies[index].vx * h
                bodies[index].y += bodies[index].vy * h
                bodies[index].angle += bodies[index].spin * h
            }
            for _ in 0..<Self.passes {
                for i in bodies.indices where bodies[i].dropped {
                    for j in bodies.indices where j > i && bodies[j].dropped {
                        collidePair(i, j)
                    }
                }
                for index in bodies.indices where bodies[index].dropped {
                    for collider in colliders { collide(index, with: collider) }
                    collideWalls(index, size: size)
                }
            }
            for index in bodies.indices {
                bodies[index].spin = min(Self.maxSpin, max(-Self.maxSpin, bodies[index].spin))
            }
        }
    }

    /// Settles a body touching a surface whose normal, pointing at the body,
    /// is (nx, ny): stops it moving in, bounces, drags along it, and turns it.
    private func contact(_ index: Int, nx: CGFloat, ny: CGFloat) {
        var body = bodies[index]
        let along = body.vx * nx + body.vy * ny
        if along < 0 {
            body.vx -= (1 + Self.bounce) * along * nx
            body.vy -= (1 + Self.bounce) * along * ny
        }
        let tx = -ny, ty = nx
        let slide = body.vx * tx + body.vy * ty
        body.vx -= slide * Self.friction * tx
        body.vy -= slide * Self.friction * ty
        body.spin += (slide / body.radius - body.spin) * Self.grip
        bodies[index] = body
    }

    private func collideWalls(_ index: Int, size: CGSize) {
        let r = bodies[index].radius
        if bodies[index].x < r {
            bodies[index].x = r
            contact(index, nx: 1, ny: 0)
        } else if bodies[index].x > size.width - r {
            bodies[index].x = size.width - r
            contact(index, nx: -1, ny: 0)
        }
        if bodies[index].y > size.height - r {
            bodies[index].y = size.height - r
            contact(index, nx: 0, ny: -1)
        } else if bodies[index].entered, bodies[index].y < r {
            bodies[index].y = r
            contact(index, nx: 0, ny: 1)
        }
        if !bodies[index].entered, bodies[index].y >= r { bodies[index].entered = true }
    }

    private func collide(_ index: Int, with box: RainCollider) {
        let rect = box.rect
        let inset = min(box.radius, rect.width / 2, rect.height / 2)
        let body = bodies[index]
        let cx = min(max(body.x, rect.minX + inset), rect.maxX - inset)
        let cy = min(max(body.y, rect.minY + inset), rect.maxY - inset)
        var dx = body.x - cx
        var dy = body.y - cy
        var distance = (dx * dx + dy * dy).squareRoot()
        let reach = body.radius + inset
        guard distance < reach else { return }
        if distance < 0.0001 {
            dx = 0
            dy = -1
            distance = 0
        } else {
            dx /= distance
            dy /= distance
        }
        let overlap = reach - distance
        bodies[index].x += dx * overlap
        bodies[index].y += dy * overlap
        contact(index, nx: dx, ny: dy)
    }

    private func collidePair(_ i: Int, _ j: Int) {
        var a = bodies[i], b = bodies[j]
        var dx = b.x - a.x, dy = b.y - a.y
        let distance = (dx * dx + dy * dy).squareRoot()
        let reach = a.radius + b.radius
        guard distance < reach, distance > 0.0001 else { return }
        dx /= distance
        dy /= distance
        let half = (reach - distance) / 2
        a.x -= dx * half
        a.y -= dy * half
        b.x += dx * half
        b.y += dy * half
        let closing = (b.vx - a.vx) * dx + (b.vy - a.vy) * dy
        if closing < 0 {
            let impulse = -(1 + Self.bounce) * closing / 2
            a.vx -= impulse * dx
            a.vy -= impulse * dy
            b.vx += impulse * dx
            b.vy += impulse * dy
            let glance = (b.vx - a.vx) * -dy + (b.vy - a.vy) * dx
            a.spin += (glance / a.radius - a.spin) * Self.pairGrip
            b.spin += (glance / b.radius - b.spin) * Self.pairGrip
        }
        bodies[i] = a
        bodies[j] = b
    }
}
